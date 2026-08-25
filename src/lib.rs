#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ---------- Memory: bump allocator ----------
const HEAP_SIZE: usize = 1 * 1024 * 1024;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut HEAP_OFFSET: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    unsafe {
        let aligned = (HEAP_OFFSET + 3) & !3;
        if aligned + size > HEAP_SIZE {
            HEAP_OFFSET = 0;
        } else {
            HEAP_OFFSET = aligned;
        }
        let ptr = core::ptr::addr_of_mut!(HEAP).cast::<u8>().add(HEAP_OFFSET);
        HEAP_OFFSET += size;
        ptr as i32
    }
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(_ptr: i32, _size: i32) {}

unsafe fn read_str<'a>(ptr: i32, len: i32) -> &'a str {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize);
        core::str::from_utf8(slice).unwrap_or("")
    }
}

// ---------- Tokenizer ----------
#[derive(Clone, Copy)]
struct TokenSpan {
    start: usize,
    end: usize,
}

const MAX_TOKENS: usize = 128;

fn is_delim_at(input: &[u8], i: usize) -> bool {
    let b = input[i];
    match b {
        b' ' | b'\n' | b'\r' | b'\t' | b'!' | b'?' | b';' | b':' | b'"' | b'(' | b')'
        | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'/' | b'\\' => true,
        b',' | b'.' => {
            let prev_digit = i > 0 && input[i - 1].is_ascii_digit();
            let next_digit = i + 1 < input.len() && input[i + 1].is_ascii_digit();
            !(prev_digit && next_digit)
        }
        _ => false,
    }
}

fn tokenize(input: &[u8], out: &mut [TokenSpan; MAX_TOKENS]) -> usize {
    let mut count = 0;
    let mut in_token = false;
    let mut start = 0;
    for i in 0..input.len() {
        let d = is_delim_at(input, i);
        if !d && !in_token {
            in_token = true;
            start = i;
        } else if d && in_token {
            in_token = false;
            if count < MAX_TOKENS {
                out[count] = TokenSpan { start, end: i };
                count += 1;
            } else {
                break;
            }
        }
    }
    if in_token && count < MAX_TOKENS {
        out[count] = TokenSpan { start, end: input.len() };
        count += 1;
    }
    count
}

fn eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() {
            return false;
        }
    }
    true
}

// ---------- Stopwords ----------
const STOPWORDS: [&[u8]; 20] = [
    b"the", b"a", b"an", b"is", b"was", b"are", b"were", b"of", b"to", b"in",
    b"on", b"at", b"and", b"or", b"it", b"this", b"that", b"be", b"as", b"by",
];

fn is_stopword(word: &[u8]) -> bool {
    for sw in STOPWORDS.iter() {
        if eq_ignore_case(word, sw) {
            return true;
        }
    }
    false
}

// ---------- Fact extraction (numbers, addresses, number-words, currency) ----------
fn is_fact_token(word: &[u8]) -> bool {
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        return true;
    }
    word.iter().any(|b| b.is_ascii_digit())
}

fn number_word_to_digits(word: &[u8]) -> Option<&'static [u8]> {
    const WORDS: [(&[u8], &[u8]); 27] = [
        (b"one", b"1"), (b"first", b"1"),
        (b"two", b"2"), (b"second", b"2"),
        (b"three", b"3"), (b"third", b"3"),
        (b"four", b"4"), (b"fourth", b"4"),
        (b"five", b"5"), (b"fifth", b"5"),
        (b"six", b"6"), (b"sixth", b"6"),
        (b"seven", b"7"), (b"seventh", b"7"),
        (b"eight", b"8"), (b"eighth", b"8"),
        (b"nine", b"9"), (b"ninth", b"9"),
        (b"ten", b"10"), (b"tenth", b"10"),
        (b"eleven", b"11"), (b"eleventh", b"11"),
        (b"twelve", b"12"), (b"twelfth", b"12"),
        (b"twenty", b"20"), (b"twentieth", b"20"),
        (b"hundred", b"100"),
    ];
    for (w, d) in WORDS.iter() {
        if eq_ignore_case(word, w) {
            return Some(d);
        }
    }
    None
}

fn is_fact_or_number_word(word: &[u8]) -> bool {
    is_fact_token(word) || number_word_to_digits(word).is_some()
}

const NUM_BUF_LEN: usize = 64;

fn canonicalize<'a>(word: &[u8], buf: &'a mut [u8; NUM_BUF_LEN]) -> &'a [u8] {
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        let m = word.len().min(NUM_BUF_LEN);
        for i in 0..m {
            buf[i] = word[i].to_ascii_lowercase();
        }
        return &buf[..m];
    }

    if let Some(d) = number_word_to_digits(word) {
        let n = d.len().min(NUM_BUF_LEN);
        buf[..n].copy_from_slice(&d[..n]);
        return &buf[..n];
    }

    let wlen = word.len();
    if wlen >= 3 {
        let tail = &word[wlen - 2..];
        if eq_ignore_case(tail, b"st")
            || eq_ignore_case(tail, b"nd")
            || eq_ignore_case(tail, b"rd")
            || eq_ignore_case(tail, b"th")
        {
            let head = &word[..wlen - 2];
            if !head.is_empty() && head.iter().all(|b| b.is_ascii_digit()) {
                let m = head.len().min(NUM_BUF_LEN);
                buf[..m].copy_from_slice(&head[..m]);
                return &buf[..m];
            }
        }
    }

    if word.iter().any(|b| b.is_ascii_digit()) {
        let mut start = 0usize;
        if start < word.len() && word[start] == b'$' {
            start += 1;
        }
        let mut end = word.len();
        let mut suffix_scale: u32 = 1;
        if end > start {
            match word[end - 1] {
                b'%' => { end -= 1; }
                b'k' | b'K' => { suffix_scale = 1_000; end -= 1; }
                b'm' | b'M' => { suffix_scale = 1_000_000; end -= 1; }
                b'b' | b'B' => { suffix_scale = 1_000_000_000; end -= 1; }
                _ => {}
            }
        }
        let has_decimal_point = start < end && word[start..end].iter().any(|&b| b == b'.');

        if !(suffix_scale > 1 && has_decimal_point) {
            let mut n = 0usize;
            for i in start..end {
                let b = word[i];
                if (b.is_ascii_digit() || b == b'.') && n < NUM_BUF_LEN {
                    buf[n] = b;
                    n += 1;
                }
            }
            if suffix_scale > 1 {
                let mut scale = suffix_scale;
                while scale > 1 {
                    if n < NUM_BUF_LEN {
                        buf[n] = b'0';
                        n += 1;
                    }
                    scale /= 10;
                }
            }
            if n > 0 {
                return &buf[..n];
            }
        }
    }

    let m = word.len().min(NUM_BUF_LEN);
    buf[..m].copy_from_slice(&word[..m]);
    &buf[..m]
}

fn eq_words(a: &[u8], b: &[u8]) -> bool {
    if eq_ignore_case(a, b) {
        return true;
    }
    let mut buf_a = [0u8; NUM_BUF_LEN];
    let mut buf_b = [0u8; NUM_BUF_LEN];
    let ca = canonicalize(a, &mut buf_a);
    let cb = canonicalize(b, &mut buf_b);
    eq_ignore_case(ca, cb)
}

fn facts_match(
    gt: &[u8], gt_tokens: &[TokenSpan],
    ma: &[u8], ma_tokens: &[TokenSpan],
) -> (usize, usize) {
    let mut total = 0usize;
    let mut matched = 0usize;
    let mut gt_buf = [0u8; NUM_BUF_LEN];
    let mut ma_buf = [0u8; NUM_BUF_LEN];
    for gt_span in gt_tokens {
        let gt_word = &gt[gt_span.start..gt_span.end];
        if !is_fact_or_number_word(gt_word) {
            continue;
        }
        total += 1;
        let gt_canon = canonicalize(gt_word, &mut gt_buf);
        for ma_span in ma_tokens {
            let ma_word = &ma[ma_span.start..ma_span.end];
            let ma_canon = canonicalize(ma_word, &mut ma_buf);
            if eq_ignore_case(gt_canon, ma_canon) {
                matched += 1;
                break;
            }
        }
    }
    (matched, total)
}

// ---------- Content words (stopwords removed) ----------
fn content_words(
    text: &[u8],
    tokens: &[TokenSpan],
    count: usize,
    out: &mut [(usize, usize); MAX_TOKENS],
) -> usize {
    let mut n = 0;
    for i in 0..count {
        let span = tokens[i];
        let word = &text[span.start..span.end];
        if !is_stopword(word) && n < MAX_TOKENS {
            out[n] = (span.start, span.end);
            n += 1;
        }
    }
    n
}

// ---------- Bigram & Unigram Dice overlap ----------
fn unigram_dice(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    if na == 0 || nb == 0 {
        return 0.0;
    }
    let mut matched = 0usize;
    for i in 0..na {
        let (s, e) = words_a[i];
        let wa = &text_a[s..e];
        for j in 0..nb {
            let (s2, e2) = words_b[j];
            let wb = &text_b[s2..e2];
            if eq_words(wa, wb) {
                matched += 1;
                break;
            }
        }
    }
    (2.0 * matched as f32) / (na + nb) as f32
}

fn bigram_dice(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    if na < 2 || nb < 2 {
        return unigram_dice(text_a, words_a, na, text_b, words_b, nb);
    }
    let bigrams_a = na - 1;
    let bigrams_b = nb - 1;
    let mut matched = 0usize;
    for i in 0..bigrams_a {
        let (a1s, a1e) = words_a[i];
        let (a2s, a2e) = words_a[i + 1];
        let w1a = &text_a[a1s..a1e];
        let w2a = &text_a[a2s..a2e];
        for j in 0..bigrams_b {
            let (b1s, b1e) = words_b[j];
            let (b2s, b2e) = words_b[j + 1];
            let w1b = &text_b[b1s..b1e];
            let w2b = &text_b[b2s..b2e];
            if eq_words(w1a, w1b) && eq_words(w2a, w2b) {
                matched += 1;
                break;
            }
        }
    }
    (2.0 * matched as f32) / (bigrams_a + bigrams_b) as f32
}

// ---------- LCS ratio on content words ----------
const LCS_CAP: usize = 64;

fn lcs_ratio(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    let na = na.min(LCS_CAP);
    let nb = nb.min(LCS_CAP);
    if na == 0 || nb == 0 {
        return 0.0;
    }

    let mut dp = [[0u16; LCS_CAP + 1]; LCS_CAP + 1];
    for i in 1..=na {
        let (s, e) = words_a[i - 1];
        let wa = &text_a[s..e];
        for j in 1..=nb {
            let (s2, e2) = words_b[j - 1];
            let wb = &text_b[s2..e2];
            if eq_words(wa, wb) {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = if dp[i - 1][j] > dp[i][j - 1] {
                    dp[i - 1][j]
                } else {
                    dp[i][j - 1]
                };
            }
        }
    }
    let lcs_len = dp[na][nb] as f32;
    let denom = ((na + nb) as f32) / 2.0;
    lcs_len / denom
}

// ---------- Negation asymmetry ----------
fn negation_count(text: &[u8], words: &[(usize, usize)], n: usize) -> u32 {
    const NEG: [&[u8]; 10] = [
        b"not", b"never", b"no", b"cannot", b"false", b"untrue",
        b"incorrect", b"denies", b"refutes", b"debunked",
    ];
    let mut count = 0u32;
    for i in 0..n {
        let (s, e) = words[i];
        let w = &text[s..e];
        for neg in NEG.iter() {
            if eq_ignore_case(w, neg) {
                count += 1;
                break;
            }
        }
        if w.len() >= 3 {
            let tail = &w[w.len() - 3..];
            if eq_ignore_case(tail, b"n't") {
                count += 1;
            }
        }
    }
    count
}

// ---------- Length-ratio penalty (anti hedge-stuffing) ----------
fn length_penalty(gt_len: usize, ma_len: usize) -> f32 {
    if ma_len == 0 {
        return 0.0;
    }
    let ratio = (gt_len as f32 * 2.0) / (ma_len as f32);
    if ratio < 1.0 { ratio } else { 1.0 }
}

// ---------- Final scoring ----------
fn score(_question: &str, ground_truth: &str, miner_answer: &str) -> f32 {
    let gt_bytes = ground_truth.as_bytes();
    let ma_bytes = miner_answer.as_bytes();

    let mut gt_tok = [TokenSpan { start: 0, end: 0 }; MAX_TOKENS];
    let mut ma_tok = [TokenSpan { start: 0, end: 0 }; MAX_TOKENS];
    let gt_count = tokenize(gt_bytes, &mut gt_tok);
    let ma_count = tokenize(ma_bytes, &mut ma_tok);

    if ma_count == 0 {
        return 0.0;
    }

    let (matched_facts, total_facts) =
        facts_match(gt_bytes, &gt_tok[..gt_count], ma_bytes, &ma_tok[..ma_count]);
    let fact_ceiling: f32 = if total_facts == 0 {
        1.0
    } else {
        matched_facts as f32 / total_facts as f32
    };

    let mut gt_content = [(0usize, 0usize); MAX_TOKENS];
    let mut ma_content = [(0usize, 0usize); MAX_TOKENS];
    let gt_c_count = content_words(gt_bytes, &gt_tok, gt_count, &mut gt_content);
    let ma_c_count = content_words(ma_bytes, &ma_tok, ma_count, &mut ma_content);

    let unigram_score = unigram_dice(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );

    let bigram_score = bigram_dice(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );

    let lcs_score = lcs_ratio(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );

    let neg_gt = negation_count(gt_bytes, &gt_content, gt_c_count);
    let neg_ma = negation_count(ma_bytes, &ma_content, ma_c_count);
    let negation_penalty: f32 = if neg_gt != neg_ma { 0.15 } else { 1.0 };

    let len_penalty = length_penalty(gt_bytes.len(), ma_bytes.len());

    let similarity = (lcs_score * 0.4) + (bigram_score * 0.3) + (unigram_score * 0.3);
    let combined = similarity * fact_ceiling * negation_penalty * len_penalty;

    combined.clamp(0.0, 1.0)
}

#[no_mangle]
pub unsafe extern "C" fn rank_answer(
    q_ptr: i32, q_len: i32,
    gt_ptr: i32, gt_len: i32,
    ma_ptr: i32, ma_len: i32,
) -> f32 {
    unsafe {
        let question = read_str(q_ptr, q_len);
        let ground_truth = read_str(gt_ptr, gt_len);
        let miner_answer = read_str(ma_ptr, ma_len);

        if miner_answer.trim().is_empty() {
            return 0.0;
        }
        if miner_answer == ground_truth {
            return 1.0;
        }

        score(question, ground_truth, miner_answer)
    }
}
