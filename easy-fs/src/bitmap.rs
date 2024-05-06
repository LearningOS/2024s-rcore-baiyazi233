use super::{get_block_cache, BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;
/// A bitmap block
/// 磁盘数据结构，它将位图区域中的一个磁盘块解释为长度为 64 的一个 u64 数组
/// 每个 u64 打包了一组 64 bits，于是整个数组包含64 X 64 = 4096bits且可以以组为单位进行操作。 
type BitmapBlock = [u64; 64];
/// Number of bits in a block
const BLOCK_BITS: usize = BLOCK_SZ * 8;
/// A bitmap
pub struct Bitmap {
    /// 所在区域的起始块编号
    start_block_id: usize,
    /// 区域内的块数
    blocks: usize,
}

/// Decompose bits into (block_pos, bits64_pos, inner_pos)
/// 将bit编号 bit 分解为区域中的块编号 block_pos 、块内的组编号 bits64_pos 以及组内编号 inner_pos 的三元组
fn decomposition(mut bit: usize) -> (usize, usize, usize) {
    let block_pos = bit / BLOCK_BITS;
    bit %= BLOCK_BITS;
    (block_pos, bit / 64, bit % 64)
}

impl Bitmap {
    /// A new bitmap from start block id and number of blocks
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }
    
    /// Allocate a new block from a block device
    pub fn alloc(&self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        // 遍历所有块，找到第一个未被使用的块
        // 找到一个尚未被全部分配出去的组，最后在里面分配一个bit
        // 它将会返回分配的bit所在的位置，等同于索引节点/数据块的编号
        // 如果所有bit均已经被分配出去了，则返回 None
        for block_id in 0..self.blocks {
            // 对当前遍历到的块，调用 get_block_cache 函数获取其对应的块缓存
            let pos = get_block_cache(
                block_id + self.start_block_id as usize,  //使用 block_id + self.start_block_id 来计算实际的块编号
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                if let Some((bits64_pos, inner_pos)) = bitmap_block
                    .iter()
                    .enumerate()
                    .find(|(_, bits64)| **bits64 != u64::MAX)
                    .map(|(bits64_pos, bits64)| (bits64_pos, bits64.trailing_ones() as usize))
                {
                    // modify cache
                    bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                    Some(block_id * BLOCK_BITS + bits64_pos * 64 + inner_pos as usize)
                } else {
                    None
                }
            });
            if pos.is_some() {
                return pos;
            }
        }
        None
    }
    
    /// Deallocate a block
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let (block_pos, bits64_pos, inner_pos) = decomposition(bit);
        get_block_cache(block_pos + self.start_block_id, Arc::clone(block_device))
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
                bitmap_block[bits64_pos] -= 1u64 << inner_pos;
            });
    }
    /// Get the max number of allocatable blocks
    pub fn maximum(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}
