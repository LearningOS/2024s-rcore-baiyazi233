use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    /// 记录该 Inode 对应的 DiskInode 保存在磁盘上的具体位置
    pub block_id: usize,
    /// 偏移量
    pub block_offset: usize,
    // 指向 EasyFileSystem 的一个指针
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }

    /// 硬链接实现
    pub fn link(&self, old: &str, new: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();  // 锁定文件系统，确保线程安全
        let op = |root_inode: &DiskInode| {  // 定义一个闭包，用于后面读取inode
            assert!(root_inode.is_dir());  // 断言：确保当前操作的是目录inode
            self.find_inode_id(old, root_inode)  // 寻找指定文件名的inode ID
        };
        if let Some(old_inode_id) = self.read_disk_inode(op) {  // 使用闭包，如果找到old的inode ID
            let new_inode_id = old_inode_id;  // 新硬链接使用相同的inode ID
            let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);  // 获取inode的位置
            self.modify_disk_inode(|root_inode| {  // 修改根目录的inode来添加新的目录项
                let file_count = (root_inode.size as usize) / DIRENT_SZ;  // 计算当前目录项的数量
                let new_size = (file_count + 1) * DIRENT_SZ;  // 计算新的目录大小
                self.increase_size(new_size as u32, root_inode, &mut fs);  // 增加根目录的大小
                let dirent = DirEntry::new(new, new_inode_id);  // 创建新的目录项结构体
                root_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );  // 在目录的尾部写入新目录项
            });
            Some(Arc::new(Self::new(
                new_inode_block_id,
                new_inode_block_offset,
                self.fs.clone(),
                self.block_device.clone(),
            )))  // 返回新创建的inode的智能指针
        } else {
            None  // 如果找不到old的inode ID，返回None
        }
    }
    

    /// 删除硬链接
    pub fn unlink(&self, name: &str) -> isize {
        let _fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        // Only when we find the path name, can we unlink it 
        if let Some(_) = self.read_disk_inode(op) {
            self.modify_disk_inode(|root_inode| {
                let mut buf = DirEntry::empty();
                let mut swap = DirEntry::empty();
                let file_count = (root_inode.size as usize) / DIRENT_SZ;
                for i in 0..file_count {
                    if root_inode.read_at(DIRENT_SZ * i, buf.as_bytes_mut(), &self.block_device) == DIRENT_SZ {
                        if buf.name() == name {
                            // we are asked not to delete the node so we overwrite the node
                            root_inode.read_at(DIRENT_SZ *(file_count - 1), swap.as_bytes_mut(), &self.block_device);
                            root_inode.write_at(DIRENT_SZ * i, swap.as_bytes_mut(), &self.block_device);
                            root_inode.size -= DIRENT_SZ as u32;
                            // unlink one per call
                            break;
                        }
                    }
                }
            });
            0
        } else {
            // cannot find the file
            -1
        }
    }

    /// get link number of thn given file
    pub fn get_link_num(&self, block_id: usize, block_offset: usize) -> u32 {
        let fs = self.fs.lock();
        let mut count = 0;
        self.read_disk_inode(|root_inode| {
            let mut buf = DirEntry::empty();
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            for i in 0..file_count {
                assert_eq!(
                    root_inode.read_at(DIRENT_SZ * i, buf.as_bytes_mut(), &self.block_device),
                    DIRENT_SZ,
                );
                let (this_inode_block_id, this_inode_block_offset) = fs.get_disk_inode_pos(buf.inode_id());
                if this_inode_block_id as usize == block_id && this_inode_block_offset == block_offset {
                    count += 1;
                }
            }
        });
        count
    }

}
