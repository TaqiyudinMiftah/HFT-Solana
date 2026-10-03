use crate::decode::DecodeError;
use meteora_dlmm_commons::{
    dlmm::accounts::{BinArray, BinArrayBitmapExtension, LbPair},
    pod_read_unaligned_skip_disc,
};

pub fn decode_lb_pair(data: &[u8]) -> Result<LbPair, DecodeError> {
    pod_read_unaligned_skip_disc::<LbPair>(data).map_err(|_| DecodeError::InvalidValue)
}

pub fn decode_bin_array(data: &[u8]) -> Result<BinArray, DecodeError> {
    pod_read_unaligned_skip_disc::<BinArray>(data).map_err(|_| DecodeError::InvalidValue)
}

pub fn decode_bitmap_extension(data: &[u8]) -> Result<BinArrayBitmapExtension, DecodeError> {
    pod_read_unaligned_skip_disc::<BinArrayBitmapExtension>(data)
        .map_err(|_| DecodeError::InvalidValue)
}
