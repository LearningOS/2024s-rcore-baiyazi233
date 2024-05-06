use core::any::Any;
/// Trait for block devices
/// which reads and writes data in the unit of blocks
pub trait BlockDevice: Send + Sync + Any {
    ///Read data form block to buffer
    /// 将编号为 block_id 的块从磁盘读入内存中的缓冲区 buf
    fn read_block(&self, block_id: usize, buf: &mut [u8]);
    ///Write data from buffer to block
    /// 将内存中的缓冲区 buf 中的数据写入磁盘编号为 block_id 的块
    fn write_block(&self, block_id: usize, buf: &[u8]);
}
