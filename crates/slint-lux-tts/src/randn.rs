//! numpy 兼容的 MT19937 + Box-Muller(极坐标法) 随机流。
//!
//! 与 numpy legacy RandomState(seed) 的 randn() 逐比特一致,
//! 用于与 Python 参考实现对拍(固定 seed 复现同一条波形)。

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

pub struct Mt19937 {
    mt: [u32; N],
    mti: usize,
}

impl Mt19937 {
    pub fn new(seed: u64) -> Self {
        // numpy RandomState(int) → mt19937_seed(seed & 0xffffffff) → init_genrand
        let mut mt = [0u32; N];
        mt[0] = (seed & 0xffff_ffff) as u32;
        for i in 1..N {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, mti: N }
    }

    fn next_u32(&mut self) -> u32 {
        if self.mti >= N {
            for i in 0..N {
                let y = (self.mt[i] & UPPER_MASK) | (self.mt[(i + 1) % N] & LOWER_MASK);
                let mut next = self.mt[(i + M) % N] ^ (y >> 1);
                if y & 1 != 0 {
                    next ^= MATRIX_A;
                }
                self.mt[i] = next;
            }
            self.mti = 0;
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// randomkit.c rk_double: 53-bit 均匀 [0,1)
    fn next_f64(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64; // 27 bits
        let b = (self.next_u32() >> 6) as f64; // 26 bits
        (a * 67108864.0 + b) / 9007199254740992.0
    }
}

/// numpy 语义的 randn 流(缓存第二个 gauss 值, 与 rk_gauss 一致)
pub struct Gauss {
    mt: Mt19937,
    cached: Option<f64>,
}

impl Gauss {
    pub fn new(seed: u64) -> Self {
        Self { mt: Mt19937::new(seed), cached: None }
    }

    pub fn next_f64(&mut self) -> f64 {
        if let Some(v) = self.cached.take() {
            return v;
        }
        let (x1, x2, r2) = loop {
            let x1 = 2.0 * self.mt.next_f64() - 1.0;
            let x2 = 2.0 * self.mt.next_f64() - 1.0;
            let r2 = x1 * x1 + x2 * x2;
            if r2 < 1.0 && r2 != 0.0 {
                break (x1, x2, r2);
            }
        };
        let f = (-2.0 * r2.ln() / r2).sqrt();
        self.cached = Some(f * x1);
        f * x2
    }

    /// 填充 f32 数组(等价 np.random.randn(n).astype(np.float32))
    pub fn fill_f32(&mut self, out: &mut [f32]) {
        for v in out.iter_mut() {
            *v = self.next_f64() as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_numpy_seed42() {
        // 基准: np.random.seed(42); np.random.randn(8)
        let expect = [
            0.496714153f64, -0.138264301, 0.647688538, 1.523029856,
            -0.234153375, -0.234136957, 1.579212816, 0.767434729,
        ];
        let mut g = Gauss::new(42);
        for e in expect {
            let v = g.next_f64();
            assert!((v - e).abs() < 1e-8, "got {v}, want {e}");
        }
    }
}
