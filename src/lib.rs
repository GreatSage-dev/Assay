#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ================================================================
//  ASSAY WASM MODULE — ZERO-ALLOCATION 4-STAGE ALGORITHM WITH
//  3 EDGE-CASE RESOLVERS:
//  1. Double Negation Resolver (Even/Odd Negation Parity)
//  2. Percent vs Decimal Equivalence (no_std parse_f32 with epsilon)
//  3. Verbosity Forgiveness (Forgive >3.0x length if anchor match >= 80%)
// ================================================================

const MAX_BUF: usize = 1024;
const MAX_TOKENS: usize = 64;
const NUM_BUF_LEN: usize = 32;

struct TokenList {
    buf: [u8; MAX_BUF],
    spans: [(usize, usize); MAX_TOKENS],
    count: usize,
}

impl TokenList {
    fn new() -> Self {
        TokenList {
            buf: [0u8; MAX_BUF],
            spans: [(0, 0); MAX_TOKENS],
            count: 0,
        }
    }

    fn get(&self, idx: usize) -> &[u8] {
        if idx >= self.count {
            &[]
        } else {
            let (s, e) = self.spans[idx];
            &self.buf[s..e]
        }
    }
}

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    for i in 0..a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() { return false; }
    }
    true
}

// ----------------------------------------------------------------
// RESOLVER 2: no_std f32 PARSER (Percent vs Decimal Equivalence)
// ----------------------------------------------------------------
fn parse_f32(token: &[u8]) -> Option<f32> {
    if token.is_empty() { return None; }
    let mut t = token;
    
    // Strip leading '$'
    if t.len() > 0 && t[0] == b'$' {
        t = &t[1..];
    }
    if t.is_empty() { return None; }

    // Check for trailing '%'
    let is_percent = t[t.len() - 1] == b'%';
    if is_percent {
        t = &t[..t.len() - 1];
    }
    if t.is_empty() { return None; }

    let mut integer_part: f32 = 0.0;
    let mut fractional_part: f32 = 0.0;
    let mut divisor: f32 = 10.0;
    let mut in_fraction = false;
    let mut digits_found = false;

    for &b in t.iter() {
        if b.is_ascii_digit() {
            digits_found = true;
            let d = (b - b'0') as f32;
            if !in_fraction {
                integer_part = integer_part * 10.0 + d;
            } else {
                fractional_part += d / divisor;
                divisor *= 10.0;
            }
        } else if b == b'.' && !in_fraction {
            in_fraction = true;
        } else {
            return None; // Invalid numeric character
        }
    }

    if !digits_found { return None; }

    let mut val = integer_part + fractional_part;
    if is_percent {
        val /= 100.0;
    }
    Some(val)
}

fn number_word_to_digit(w: &[u8]) -> Option<&'static [u8]> {
    const WORDS: [(&[u8], &[u8]); 27] = [
        (b"one", b"1"), (b"first", b"1"), (b"two", b"2"), (b"second", b"2"),
        (b"three", b"3"), (b"third", b"3"), (b"four", b"4"), (b"fourth", b"4"),
        (b"five", b"5"), (b"fifth", b"5"), (b"six", b"6"), (b"sixth", b"6"),
        (b"seven", b"7"), (b"seventh", b"7"), (b"eight", b"8"), (b"eighth", b"8"),
        (b"nine", b"9"), (b"ninth", b"9"), (b"ten", b"10"), (b"tenth", b"10"),
        (b"eleven", b"11"), (b"eleventh", b"11"), (b"twelve", b"12"), (b"twelfth", b"12"),
        (b"twenty", b"20"), (b"twentieth", b"20"), (b"hundred", b"100"),
    ];
    for (wrd, d) in WORDS.iter() {
        if eq_ci(w, wrd) {
            return Some(d);
        }
    }
    None
}

fn canonicalize<'a>(word: &[u8], buf: &'a mut [u8; NUM_BUF_LEN]) -> &'a [u8] {
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        let m = word.len().min(NUM_BUF_LEN);
        for i in 0..m { buf[i] = word[i].to_ascii_lowercase(); }
        return &buf[..m];
    }
    if let Some(d) = number_word_to_digit(word) {
        let n = d.len().min(NUM_BUF_LEN);
        buf[..n].copy_from_slice(&d[..n]);
        return &buf[..n];
    }
    let wl = word.len();
    if wl >= 3 {
        let tail = &word[wl - 2..];
        if eq_ci(tail, b"st") || eq_ci(tail, b"nd") || eq_ci(tail, b"rd") || eq_ci(tail, b"th") {
            let head = &word[..wl - 2];
            if !head.is_empty() && head.iter().all(|b| b.is_ascii_digit()) {
                let m = head.len().min(NUM_BUF_LEN);
                buf[..m].copy_from_slice(&head[..m]);
                return &buf[..m];
            }
        }
    }
    if word.iter().any(|b| b.is_ascii_digit()) {
        let s = if word[0] == b'$' { 1 } else { 0 };
        let mut e = wl;
        let mut scale: u32 = 1;
        if e > s {
            match word[e - 1] {
                b'k' | b'K' => { scale = 1_000; e -= 1; }
                b'm' | b'M' => { scale = 1_000_000; e -= 1; }
                b'b' | b'B' => { scale = 1_000_000_000; e -= 1; }
                b'%' => { e -= 1; }
                _ => {}
            }
        }
        let has_dot = word[s..e].iter().any(|&b| b == b'.');
        if !(scale > 1 && has_dot) {
            let mut n = 0;
            for i in s..e {
                if (word[i].is_ascii_digit() || word[i] == b'.') && n < NUM_BUF_LEN {
                    buf[n] = word[i];
                    n += 1;
                }
            }
            if scale > 1 {
                let mut sc = scale;
                while sc > 1 {
                    if n < NUM_BUF_LEN { buf[n] = b'0'; n += 1; }
                    sc /= 10;
                }
            }
            if n > 0 { return &buf[..n]; }
        }
    }
    let m = wl.min(NUM_BUF_LEN);
    buf[..m].copy_from_slice(&word[..m]);
    &buf[..m]
}

fn normalize_and_tokenize(input: &[u8], out: &mut TokenList) {
    let mut buf_idx = 0usize;
    let mut in_token = false;
    let mut token_start = 0usize;

    let len = input.len();
    let mut i = 0usize;

    while i < len && buf_idx < MAX_BUF && out.count < MAX_TOKENS {
        let b = input[i];
        let lower = b.to_ascii_lowercase();

        let is_alnum = lower.is_ascii_alphanumeric();

        // Strip commas from numbers: 192,841 -> 192841
        let is_comma_in_num = lower == b',' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_dot = lower == b'.' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_percent = lower == b'%';

        let is_hex_x = (lower == b'x' || lower == b'X') && {
            i > 0 && input[i - 1] == b'0' && (i == 1 || !input[i - 2].is_ascii_alphanumeric())
        };

        let is_valid = is_alnum || is_dot || is_hex_x || is_percent;

        if is_comma_in_num {
            // Ignore comma inside numbers
        } else if is_valid {
            if !in_token {
                in_token = true;
                token_start = buf_idx;
            }
            out.buf[buf_idx] = lower;
            buf_idx += 1;
        } else {
            if in_token {
                in_token = false;
                if out.count < MAX_TOKENS {
                    out.spans[out.count] = (token_start, buf_idx);
                    out.count += 1;
                }
            }
        }
        i += 1;
    }

    if in_token && out.count < MAX_TOKENS && buf_idx <= MAX_BUF {
        out.spans[out.count] = (token_start, buf_idx);
        out.count += 1;
    }
}

fn is_number_or_hex(token: &[u8]) -> bool {
    if token.len() >= 2 && token[0] == b'0' && token[1] == b'x' {
        return true;
    }
    if number_word_to_digit(token).is_some() {
        return true;
    }
    for &b in token.iter() {
        if b.is_ascii_digit() {
            return true;
        }
    }
    false
}

fn is_substring(needle: &[u8], haystack: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > haystack.len() { return false; }
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let mut ok = true;
        for j in 0..needle.len() {
            if haystack[i + j] != needle[j] {
                ok = false;
                break;
            }
        }
        if ok { return true; }
        i += 1;
    }
    false
}

fn match_tokens(a: &[u8], b: &[u8]) -> bool {
    // RESOLVER 2: Percent vs Decimal Equivalence
    if let (Some(v1), Some(v2)) = (parse_f32(a), parse_f32(b)) {
        let diff = if v1 > v2 { v1 - v2 } else { v2 - v1 };
        if diff < 0.001 {
            return true;
        }
    }

    if is_substring(a, b) || is_substring(b, a) { return true; }

    let mut buf_a = [0u8; NUM_BUF_LEN];
    let mut buf_b = [0u8; NUM_BUF_LEN];
    let ca = canonicalize(a, &mut buf_a);
    let cb = canonicalize(b, &mut buf_b);
    eq_ci(ca, cb)
}

// ----------------------------------------------------------------
// RESOLVER 1: THE DOUBLE NEGATION RESOLVER (Odd/Even Parity)
// ----------------------------------------------------------------
const NEGATION_WORDS: [&[u8]; 8] = [
    b"not", b"never", b"false", b"fake", b"disputed", b"debunked", b"incorrect", b"untrue",
];

fn count_negations(tokens: &TokenList) -> usize {
    let mut count = 0usize;
    for i in 0..tokens.count {
        let tok = tokens.get(i);
        for neg in NEGATION_WORDS.iter() {
            if is_substring(neg, tok) {
                count += 1;
                break;
            }
        }
        if tok.len() >= 3 && is_substring(b"n't", tok) {
            count += 1;
        }
    }
    count
}

fn evaluate(gt_bytes: &[u8], ma_bytes: &[u8]) -> f32 {
    let mut gt_tokens = TokenList::new();
    let mut ma_tokens = TokenList::new();

    normalize_and_tokenize(gt_bytes, &mut gt_tokens);
    normalize_and_tokenize(ma_bytes, &mut ma_tokens);

    if gt_tokens.count == 0 {
        return if ma_tokens.count == 0 { 1.0 } else { 0.0 };
    }
    if ma_tokens.count == 0 {
        return 0.0;
    }

    // STAGE 4: DENSITY-WEIGHTED SUBSTRING OVERLAP
    let mut matched_gt_substrings = 0usize;
    for i in 0..gt_tokens.count {
        let gt_w = gt_tokens.get(i);
        let mut matched = false;
        for j in 0..ma_tokens.count {
            let ma_w = ma_tokens.get(j);
            if match_tokens(gt_w, ma_w) {
                matched = true;
                break;
            }
        }
        if matched {
            matched_gt_substrings += 1;
        }
    }

    let mut base_score = matched_gt_substrings as f32 / gt_tokens.count as f32;

    // RESOLVER 3: VERBOSITY FORGIVENESS & FLUFF DAMPENING
    // Forgive normal length variations (up to 3.5x), but dampen extreme walls of text (>3.5x)
    let verbosity_ratio = ma_tokens.count as f32 / gt_tokens.count as f32;
    if verbosity_ratio > 3.5 || (verbosity_ratio > 2.0 && base_score < 0.80) {
        base_score *= 2.0 / verbosity_ratio;
    }

    // STAGE 2: THE NUMBER & HASH INVARIANT (Hard Filter)
    let mut missing_numbers = 0usize;
    for i in 0..gt_tokens.count {
        let gt_w = gt_tokens.get(i);
        if is_number_or_hex(gt_w) {
            let mut found = false;
            for j in 0..ma_tokens.count {
                let ma_w = ma_tokens.get(j);
                if match_tokens(gt_w, ma_w) {
                    found = true;
                    break;
                }
            }
            if !found {
                missing_numbers += 1;
            }
        }
    }

    let number_penalty = missing_numbers as f32 * 1.0;

    // ----------------------------------------------------------------
    // RESOLVER 1: DOUBLE NEGATION PARITY CHECK
    // If GT and Miner both have EVEN (e.g. 0 or 2) or both have ODD,
    // double negations cancel out -> NO PENALTY!
    // ----------------------------------------------------------------
    let gt_neg_count = count_negations(&gt_tokens);
    let ma_neg_count = count_negations(&ma_tokens);

    let gt_is_odd = (gt_neg_count % 2) == 1;
    let ma_is_odd = (ma_neg_count % 2) == 1;

    let polarity_penalty = if gt_is_odd != ma_is_odd {
        1.0 // Only penalize true odd-parity contradictions
    } else {
        0.0 // Even parity (0 or 2 negations like "not false") -> NO PENALTY!
    };

    let final_score = base_score - number_penalty - polarity_penalty;

    if final_score < 0.0 {
        0.0
    } else if final_score > 1.0 {
        1.0
    } else {
        final_score
    }
}

unsafe fn read_slice<'a>(ptr: i32, len: i32) -> &'a [u8] {
    if ptr == 0 || len <= 0 {
        &[]
    } else {
        core::slice::from_raw_parts(ptr as *const u8, len as usize)
    }
}

#[no_mangle]
pub unsafe extern "C" fn rank_answer(
    _q_ptr: i32, _q_len: i32,
    gt_ptr: i32, gt_len: i32,
    ma_ptr: i32, ma_len: i32,
) -> f32 {
    let gt = read_slice(gt_ptr, gt_len);
    let ma = read_slice(ma_ptr, ma_len);

    if ma.is_empty() { return 0.0; }
    if gt == ma { return 1.0; }

    evaluate(gt, ma)
}

static mut BUMP_HEAP: [u8; 64 * 1024] = [0u8; 64 * 1024];
static mut BUMP_OFFSET: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    unsafe {
        let aligned = (BUMP_OFFSET + 7) & !7;
        if aligned + size > 64 * 1024 {
            BUMP_OFFSET = 0;
            core::ptr::addr_of_mut!(BUMP_HEAP).cast::<u8>() as i32
        } else {
            let ptr = core::ptr::addr_of_mut!(BUMP_HEAP).cast::<u8>().add(aligned);
            BUMP_OFFSET = aligned + size;
            ptr as i32
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(_ptr: i32, _size: i32) {}
