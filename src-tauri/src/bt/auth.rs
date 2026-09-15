// Authentication challenge-response: custom Bluetooth SAFER+ (128-bit key, 8 rounds).
// Ported from Gadgetbridge `service/devices/redmibuds/protocol/{Authentication,AuthData}.java`.
// All arithmetic is mod-256 byte arithmetic (Java byte semantics).

const PATTERN: u32 = 0x9999; // per-byte-bit xor-vs-add selector
const BLOCK_SIZE: usize = 16;

pub const SEQ: [u8; BLOCK_SIZE] = [
    0x11, 0x22, 0x33, 0x33, 0x22, 0x11, 0x11, 0x22, 0x33, 0x33, 0x22, 0x11, 0x11, 0x22, 0x33, 0x33,
];

#[rustfmt::skip]
const COEFFICIENTS: [[i32; BLOCK_SIZE]; BLOCK_SIZE] = [
    [2,1,1,1,4,2,1,1,2,2,4,2,4,4,16,8],
    [2,1,1,1,4,2,1,1,1,1,2,1,2,2,8,4],
    [1,1,4,2,2,2,4,2,16,8,4,4,2,1,1,1],
    [1,1,4,2,1,1,2,1,8,4,2,2,2,1,1,1],
    [16,8,2,2,4,2,4,4,1,1,4,2,1,1,2,1],
    [8,4,1,1,2,1,2,2,1,1,4,2,1,1,2,1],
    [2,2,4,2,4,4,16,8,2,1,1,1,4,2,1,1],
    [1,1,2,1,2,2,8,4,2,1,1,1,4,2,1,1],
    [4,2,4,4,16,8,2,2,1,1,2,1,1,1,4,2],
    [2,1,2,2,8,4,1,1,1,1,2,1,1,1,4,2],
    [4,4,16,8,1,1,2,1,4,2,1,1,4,2,2,2],
    [2,2,8,4,1,1,2,1,4,2,1,1,2,1,1,1],
    [1,1,2,1,1,1,4,2,4,4,16,8,2,2,4,2],
    [1,1,2,1,1,1,4,2,2,2,8,4,1,1,2,1],
    [4,2,1,1,2,1,1,1,4,2,2,2,16,8,4,4],
    [4,2,1,1,2,1,1,1,2,1,1,1,8,4,2,2],
];

struct SaferPlus {
    bias_matrix: [[u8; BLOCK_SIZE]; BLOCK_SIZE],
    exp_tab: [u8; 256],
    log_tab: [u8; 256],
}

impl SaferPlus {
    fn new() -> Self {
        let mut bias_matrix = [[0u8; BLOCK_SIZE]; BLOCK_SIZE];
        for (i, bias_vec) in bias_matrix.iter_mut().enumerate() {
            for (j, val) in bias_vec.iter_mut().enumerate() {
                let exponent = 17 * ((i + 2) as u32) + (j as u32 + 1);
                // 45^(45^exponent mod 257) mod 257, done with u128 to avoid overflow.
                let inner = pow_mod(45, exponent as u128, 257);
                let outer = pow_mod(45, inner, 257);
                *val = to_byte(outer);
            }
        }

        let mut exp_tab = [0u8; 256];
        for (i, e) in exp_tab.iter_mut().enumerate() {
            let exp = pow_mod(45, i as u128, 257);
            *e = if i == 128 { 0 } else { to_byte(exp) };
        }

        let mut log_tab = [128u8; 256];
        for i in 1..256u32 {
            let exp = pow_mod(45, i as u128, 257) as usize;
            if exp != 256 {
                log_tab[exp] = to_byte(i as u128);
            }
        }

        SaferPlus {
            bias_matrix,
            exp_tab,
            log_tab,
        }
    }

    fn key_schedule(&self, key_init: &[u8; BLOCK_SIZE]) -> Vec<[u8; BLOCK_SIZE]> {
        let mut key_init = *key_init;
        key_init[15] ^= 6;

        // 17-byte register: key bytes + xor of all.
        let mut register = [0u8; 17];
        register[..16].copy_from_slice(&key_init);
        register[16] = key_init.iter().fold(0u8, |acc, &b| acc ^ b);

        let mut keys = vec![[0u8; BLOCK_SIZE]; 17];
        keys[0] = key_init;

        for key_idx in 1..17usize {
            for b in register.iter_mut() {
                *b = b.rotate_left(5);
            }
            for (i, k) in keys[key_idx].iter_mut().enumerate() {
                *k = register[(key_idx + i) % 17].wrapping_add(self.bias_matrix[key_idx - 1][i]);
            }
        }
        keys
    }

    fn encrypt(&self, plaintext: &[u8; BLOCK_SIZE], keys: &[[u8; BLOCK_SIZE]; 17]) -> [u8; BLOCK_SIZE] {
        let mut ciphertext = *plaintext;

        for round in 0..8usize {
            if round == 2 {
                for i in 0..BLOCK_SIZE {
                    ciphertext[i] = xor_or_add(ciphertext[i], plaintext[i], i);
                }
            }
            for i in 0..BLOCK_SIZE {
                ciphertext[i] = xor_or_add(ciphertext[i], keys[round * 2][i], i);
            }
            for i in 0..BLOCK_SIZE {
                ciphertext[i] = if pattern_bit(i) {
                    self.exp_tab[ciphertext[i] as usize]
                } else {
                    self.log_tab[ciphertext[i] as usize]
                };
            }
            for i in 0..BLOCK_SIZE {
                ciphertext[i] = if pattern_bit(i) {
                    keys[round * 2 + 1][i].wrapping_add(ciphertext[i])
                } else {
                    keys[round * 2 + 1][i] ^ ciphertext[i]
                };
            }
            // Multiply by the coefficient matrix, mod 256.
            let copy = ciphertext;
            for (i, c) in ciphertext.iter_mut().enumerate() {
                let mut sum: i32 = 0;
                for (j, &cc) in copy.iter().enumerate() {
                    sum += COEFFICIENTS[i][j] * (cc as i8 as i32);
                }
                *c = (sum as u8) & 0xFF;
            }
        }

        for i in 0..BLOCK_SIZE {
            ciphertext[i] = if pattern_bit(i) {
                keys[16][i] ^ ciphertext[i]
            } else {
                keys[16][i].wrapping_add(ciphertext[i])
            };
        }
        ciphertext
    }
}

fn pattern_bit(i: usize) -> bool {
    PATTERN & (1 << i) != 0
}

fn xor_or_add(a: u8, b: u8, i: usize) -> u8 {
    if pattern_bit(i) {
        a ^ b
    } else {
        a.wrapping_add(b)
    }
}

/// Java `(byte)` cast semantics: value mod 256 as signed byte.
fn to_byte(v: u128) -> u8 {
    (v % 256) as u8
}

fn pow_mod(mut base: u128, mut exp: u128, modulus: u128) -> u128 {
    let mut result = 1u128;
    base %= modulus;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result * base % modulus;
        }
        base = base * base % modulus;
        exp >>= 1;
    }
    result
}

/// Compute the response to the buds' 16-byte auth challenge.
pub fn compute_challenge_response(challenge: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
    let sp = SaferPlus::new();
    let keys = sp.key_schedule(challenge);
    let mut keys_arr = [[0u8; BLOCK_SIZE]; 17];
    keys_arr.copy_from_slice(&keys);
    sp.encrypt(&SEQ, &keys_arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_is_deterministic() {
        let challenge = [1u8; 16];
        let r1 = compute_challenge_response(&challenge);
        let r2 = compute_challenge_response(&challenge);
        assert_eq!(r1, r2);
        assert_ne!(r1, [0u8; 16]);
    }
}
