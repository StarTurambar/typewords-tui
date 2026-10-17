//! 可注入的随机源（测试用固定种子）。

use rand::prelude::*;
use rand::rngs::StdRng;

pub type Rng = StdRng;

pub fn seeded(seed: u64) -> Rng {
    StdRng::seed_from_u64(seed)
}

pub fn random_seed() -> u64 {
    rand::random()
}

/// Fisher–Yates 洗牌。
pub fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    if items.len() < 2 {
        return;
    }
    for index in (1..items.len()).rev() {
        let swap_with = rng.random_range(0..=index);
        items.swap(index, swap_with);
    }
}

/// 取一个 [0, len) 的下标；len 为 0 时返回 0。
pub fn pick_index(rng: &mut Rng, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        rng.random_range(0..len)
    }
}
