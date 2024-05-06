use super::{BlockDevice, BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use spin::Mutex;
/// Cached block inside memory
pub struct BlockCache {
    /// cached block data
    /// 位于内存中的缓冲区,512字节的数组
    cache: [u8; BLOCK_SZ],
    /// underlying block id
    /// 缓冲区对应的磁盘块编号
    block_id: usize,
    /// underlying block device
    /// 一个底层块设备的引用，可通过它进行块读写
    block_device: Arc<dyn BlockDevice>,
    /// whether the block is dirty
    /// 标记缓冲区是否被修改过
    modified: bool,
}

impl BlockCache {
    /// Load a new BlockCache from disk.
    /// 创建 BlockCache 时，将一个块从磁盘读到缓冲区 cache
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SZ];
        // 从磁盘读取编号为 block_id 的块到缓冲区中
        block_device.read_block(block_id, &mut cache);
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
        // 一旦磁盘块已经存在于内存缓存中，CPU 就可以直接访问磁盘块数据
    }
    /// Get the address of an offset inside the cached block data
    /// 获取缓冲区中偏移量为 offset 的地址
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    /// 获取缓冲区中偏移量为 offset 的数据类型为 T 的不可变引用
    /// Trait Bound 限制类型 T 必须是一个编译时已知大小的类型
    pub fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        // 获取类型 T 的大小
        let type_size = core::mem::size_of::<T>();
        // 确保偏移量 offset 加上类型 T 的大小不会超过缓冲区的大小
        assert!(offset + type_size <= BLOCK_SZ);
        // 返回偏移量为 offset 的地址，并将其转换为类型 T 的不可变引用
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    /// 获取缓冲区中偏移量为 offset 的数据类型为 T 的可变引用
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        // 标记缓冲区已经被修改过
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    /// 从缓冲区中偏移量为 offset 的位置不可变读取数据，并通过闭包 f 处理数据
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    /// 从缓冲区中偏移量为 offset 的位置可变读取数据，并通过闭包 f 处理数据
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    /// 如果自身确实被修改过的话才会将缓冲区的内容写回磁盘
    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}
/// Use a block cache of 16 blocks
/// 为了避免在块缓存上浪费过多内存，我们希望内存中同时只能驻留有限个磁盘块的缓冲区
const BLOCK_CACHE_SIZE: usize = 16;

/// Block cache manager
pub struct BlockCacheManager {
    /// 块编号和块缓存的二元组队列
    queue: VecDeque<(usize, Arc<Mutex<BlockCache>>)>,
}

impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// 从块缓存管理器中获取一个编号为 block_id 的块的块缓存
    /// 如果缓存中已经存在编号为 block_id 的块，则直接返回该块的缓存
    /// 如果找不到，会从磁盘读取到内存中，还有可能会发生缓存替换
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        // 整个队列试图找到一个编号相同的块缓存
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            // hit
            Arc::clone(&pair.1)
        } else {
            // substitute
            // 达到了上限，需要替换
            if self.queue.len() == BLOCK_CACHE_SIZE {
                // from front to tail
                if let Some((idx, _)) = self
                    .queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)   //该元素的引用计数为 1
                {
                    self.queue.drain(idx..=idx);
                } else {
                    panic!("Run out of BlockCache!");
                }
            }
            // load block into mem and push back
            // 创建一个新的块缓存
            let block_cache = Arc::new(Mutex::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            // 将新的块缓存加入到队列尾部
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }
}

lazy_static! {
    /// The global block cache manager
    pub static ref BLOCK_CACHE_MANAGER: Mutex<BlockCacheManager> =
        Mutex::new(BlockCacheManager::new());
}

/// Get the block cache corresponding to the given block id and block device
/// 请求块缓存
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    BLOCK_CACHE_MANAGER
        .lock()
        .get_block_cache(block_id, block_device)
}

/// Sync all block cache to block device
pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for (_, cache) in manager.queue.iter() {
        cache.lock().sync();
    }
}
