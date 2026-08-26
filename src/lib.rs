#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ================================================================
//  ASSAY v10 — ULTIMATE CHAMPION-SLAYER SCORER
//  Zero heap allocations, 100% stack-based.
//
//  Root Cause Fixes (v9 → v10):
//  1. Expanded Polarity Dictionary: 11 → 30 words per list.
//     Catches antonyms like earned/lost, rose/fell, safe/compromised.
//  2. Guarded Substring Matching: Blocks false positives where
//     "success" matched "unsuccessful" via substring.
//  3. Synonym Table: 20 curated pairs for paraphrase recall boost.
//  4. Expanded Stopwords: 20 → 30 words for cleaner recall.
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
    let mut i = 0;
    while i < a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() { return false; }
        i += 1;
    }
    true
}

fn is_substring(needle: &[u8], haystack: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > haystack.len() { return false; }
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let mut ok = true;
        let mut j = 0;
        while j < needle.len() {
            if haystack[i + j] != needle[j] {
                ok = false;
                break;
            }
            j += 1;
        }
        if ok { return true; }
        i += 1;
    }
    false
}

// ========== 1. POLARITY & ANTONYM ENGINE (EXPANDED) ==========

// Antonym pairs: if GT has word A and MA has word B (or vice versa), it's a contradiction
const ANTONYM_PAIRS: [(&[u8], &[u8]); 30] = [
    (b"true", b"false"),
    (b"correct", b"incorrect"),
    (b"valid", b"invalid"),
    (b"succeeded", b"failed"),
    (b"success", b"failure"),
    (b"passed", b"failed"),
    (b"successful", b"unsuccessful"),
    (b"profit", b"loss"),
    (b"gain", b"loss"),
    (b"increase", b"decrease"),
    (b"approved", b"denied"),
    (b"approved", b"rejected"),
    (b"confirmed", b"denied"),
    (b"accepted", b"rejected"),
    (b"earned", b"lost"),
    (b"won", b"lost"),
    (b"rose", b"fell"),
    (b"rose", b"dropped"),
    (b"grew", b"fell"),
    (b"grew", b"dropped"),
    (b"completed", b"failed"),
    (b"safe", b"compromised"),
    (b"safe", b"unsafe"),
    (b"secure", b"vulnerable"),
    (b"active", b"inactive"),
    (b"available", b"unavailable"),
    (b"enabled", b"disabled"),
    (b"improved", b"dropped"),
    (b"recovered", b"crashed"),
    (b"verified", b"denied"),
];

const POSITIVE_WORDS: [&[u8]; 25] = [
    b"true", b"correct", b"valid", b"succeeded", b"success",
    b"passed", b"successful", b"profit", b"gain", b"increase",
    b"approved", b"confirmed", b"accepted", b"earned", b"won",
    b"rose", b"grew", b"jumped", b"completed", b"working",
    b"safe", b"secure", b"improved", b"recovered", b"verified",
];

const NEGATIVE_WORDS: [&[u8]; 30] = [
    b"false", b"incorrect", b"invalid", b"failed", b"failure",
    b"reverted", b"loss", b"drop", b"decrease", b"denied",
    b"fake", b"rejected", b"lost", b"crashed", b"broke",
    b"broken", b"hacked", b"exploited", b"compromised", b"fell",
    b"dropped", b"attacked", b"bankrupt", b"unsafe", b"vulnerable",
    b"inactive", b"unavailable", b"disabled", b"destroyed", b"eliminated",
];

const NEGATION_MODIFIERS: [&[u8]; 6] = [
    b"not", b"never", b"no", b"cannot", b"neither", b"without",
];

// Check if GT and MA contain an antonym pair (direct word swap contradiction)
// Respects negation: "not false" cancels the antonym (double negation = positive)
fn has_antonym_contradiction(gt_tokens: &TokenList, ma_tokens: &TokenList) -> bool {
    let mut pi = 0;
    while pi < ANTONYM_PAIRS.len() {
        let (word_a, word_b) = ANTONYM_PAIRS[pi];

        // Check: GT has word_a AND MA has word_b (or vice versa)
        let mut gt_has_a = false;
        let mut gt_has_b = false;

        let mut i = 0;
        while i < gt_tokens.count {
            let tok = gt_tokens.get(i);
            if eq_ci(tok, word_a) { gt_has_a = true; }
            if eq_ci(tok, word_b) { gt_has_b = true; }
            i += 1;
        }

        // For MA, also check if the antonym word is negated (adjacent negation modifier)
        let mut ma_has_b_unnegated = false;
        let mut ma_has_a_unnegated = false;
        let mut ma_has_a = false;
        let mut ma_has_b = false;

        i = 0;
        while i < ma_tokens.count {
            let tok = ma_tokens.get(i);
            if eq_ci(tok, word_b) {
                ma_has_b = true;
                // Check if negated: look at 1-2 tokens before for negation modifier
                if !is_negated_at(ma_tokens, i) {
                    ma_has_b_unnegated = true;
                }
            }
            if eq_ci(tok, word_a) {
                ma_has_a = true;
                if !is_negated_at(ma_tokens, i) {
                    ma_has_a_unnegated = true;
                }
            }
            i += 1;
        }

        // Contradiction only if the antonym word in MA is NOT negated
        if gt_has_a && ma_has_b_unnegated && !ma_has_a {
            return true;
        }
        if gt_has_b && ma_has_a_unnegated && !ma_has_b {
            return true;
        }

        pi += 1;
    }
    false
}

// Check if the token at position `pos` is preceded by a negation modifier (within 2 tokens)
fn is_negated_at(tokens: &TokenList, pos: usize) -> bool {
    // Check 1 token before
    if pos >= 1 {
        let prev = tokens.get(pos - 1);
        let mut j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(prev, NEGATION_MODIFIERS[j]) { return true; }
            j += 1;
        }
        if is_substring(b"n't", prev) { return true; }
    }
    // Check 2 tokens before (e.g., "is not entirely false")
    if pos >= 2 {
        let prev2 = tokens.get(pos - 2);
        let mut j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(prev2, NEGATION_MODIFIERS[j]) { return true; }
            j += 1;
        }
        if is_substring(b"n't", prev2) { return true; }
    }
    false
}

fn compute_net_polarity(tokens: &TokenList) -> i8 {
    let mut has_pos = false;
    let mut has_neg = false;
    let mut neg_count = 0usize;

    let mut i = 0;
    while i < tokens.count {
        let tok = tokens.get(i);

        let mut j = 0;
        while j < POSITIVE_WORDS.len() {
            if eq_ci(tok, POSITIVE_WORDS[j]) { has_pos = true; break; }
            j += 1;
        }

        j = 0;
        while j < NEGATIVE_WORDS.len() {
            if eq_ci(tok, NEGATIVE_WORDS[j]) { has_neg = true; break; }
            j += 1;
        }

        let mut found_neg = false;
        j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(tok, NEGATION_MODIFIERS[j]) { found_neg = true; break; }
            j += 1;
        }
        if !found_neg && is_substring(b"n't", tok) { found_neg = true; }
        if found_neg { neg_count += 1; }

        i += 1;
    }

    let is_odd = (neg_count % 2) == 1;

    if has_pos && !is_odd { return 1; }
    if has_pos && is_odd { return -1; }
    if has_neg && !is_odd { return -1; }
    if has_neg && is_odd { return 1; }
    0
}

// ========== 2. CANONICAL FACT EXTRACTION ENGINE ==========
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
    let mut i = 0;
    while i < WORDS.len() {
        if eq_ci(w, WORDS[i].0) {
            return Some(WORDS[i].1);
        }
        i += 1;
    }
    None
}

fn canonicalize_fact<'a>(word: &[u8], buf: &'a mut [u8; NUM_BUF_LEN]) -> &'a [u8] {
    // Hex string
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        let m = word.len().min(NUM_BUF_LEN);
        let mut i = 0;
        while i < m { buf[i] = word[i].to_ascii_lowercase(); i += 1; }
        return &buf[..m];
    }

    // Ordinal / number word
    if let Some(d) = number_word_to_digit(word) {
        let n = d.len().min(NUM_BUF_LEN);
        buf[..n].copy_from_slice(&d[..n]);
        return &buf[..n];
    }

    // Ordinal suffix 3rd -> 3
    let wl = word.len();
    if wl >= 3 {
        let tail = &word[wl - 2..];
        if eq_ci(tail, b"st") || eq_ci(tail, b"nd") || eq_ci(tail, b"rd") || eq_ci(tail, b"th") {
            let head = &word[..wl - 2];
            if !head.is_empty() {
                let mut all_digit = true;
                let mut k = 0;
                while k < head.len() {
                    if !head[k].is_ascii_digit() { all_digit = false; break; }
                    k += 1;
                }
                if all_digit {
                    let m = head.len().min(NUM_BUF_LEN);
                    buf[..m].copy_from_slice(&head[..m]);
                    return &buf[..m];
                }
            }
        }
    }

    // Currency / magnitude ($10M -> 10000000)
    let mut has_digit = false;
    {
        let mut k = 0;
        while k < word.len() {
            if word[k].is_ascii_digit() { has_digit = true; break; }
            k += 1;
        }
    }
    if has_digit {
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
        let mut has_dot = false;
        {
            let mut k = s;
            while k < e {
                if word[k] == b'.' { has_dot = true; break; }
                k += 1;
            }
        }
        if !(scale > 1 && has_dot) {
            let mut n = 0;
            let mut k = s;
            while k < e {
                if (word[k].is_ascii_digit() || word[k] == b'.') && n < NUM_BUF_LEN {
                    buf[n] = word[k];
                    n += 1;
                }
                k += 1;
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

fn is_fact_token(token: &[u8]) -> bool {
    if token.len() >= 2 && token[0] == b'0' && (token[1] == b'x' || token[1] == b'X') {
        return true;
    }
    if number_word_to_digit(token).is_some() {
        return true;
    }
    let mut i = 0;
    while i < token.len() {
        if token[i].is_ascii_digit() { return true; }
        i += 1;
    }
    false
}

fn facts_all_satisfied(gt_tokens: &TokenList, ma_tokens: &TokenList) -> bool {
    let mut gt_buf = [0u8; NUM_BUF_LEN];
    let mut ma_buf = [0u8; NUM_BUF_LEN];

    let mut i = 0;
    while i < gt_tokens.count {
        let gt_tok = gt_tokens.get(i);
        if !is_fact_token(gt_tok) { i += 1; continue; }

        let gt_canon = canonicalize_fact(gt_tok, &mut gt_buf);
        let mut matched = false;

        let mut j = 0;
        while j < ma_tokens.count {
            let ma_tok = ma_tokens.get(j);
            let ma_canon = canonicalize_fact(ma_tok, &mut ma_buf);
            if eq_ci(gt_canon, ma_canon) {
                matched = true;
                break;
            }
            j += 1;
        }

        if !matched {
            return false;
        }
        i += 1;
    }
    true
}

// ========== 3. TOKENIZATION & NORMALIZATION ==========
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

        // Strip commas inside numbers: 192,841 -> 192841
        let is_comma_in_num = lower == b',' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_dot = lower == b'.' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_percent = lower == b'%';

        let is_hex_x = (lower == b'x') && {
            i > 0 && input[i - 1] == b'0' && (i == 1 || !input[i - 2].is_ascii_alphanumeric())
        };

        let is_valid = is_alnum || is_dot || is_hex_x || is_percent;

        if is_comma_in_num {
            // Skip comma inside number
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

// ========== 4. CONTENT RECALL WITH IMPROVED MATCHING ==========

// Expanded stopwords (30 words)
const STOPWORDS: [&[u8]; 30] = [
    b"the", b"a", b"an", b"is", b"was", b"are", b"were", b"of", b"to", b"in",
    b"on", b"at", b"and", b"or", b"it", b"this", b"that", b"be", b"as", b"by",
    b"for", b"with", b"has", b"had", b"have", b"been", b"from", b"about", b"into", b"its",
];

fn is_stopword(tok: &[u8]) -> bool {
    let mut i = 0;
    while i < STOPWORDS.len() {
        if eq_ci(tok, STOPWORDS[i]) { return true; }
        i += 1;
    }
    false
}

// Synonym table: pairs of words that are semantically equivalent
// but where substring matching wouldn't help
const SYNONYM_PAIRS: [(&[u8], &[u8]); 22] = [
    (b"transaction", b"tx"),
    (b"succeeded", b"completed"),
    (b"succeeded", b"passed"),
    (b"hacked", b"exploited"),
    (b"hacked", b"breached"),
    (b"hacked", b"compromised"),
    (b"confirmed", b"verified"),
    (b"approved", b"accepted"),
    (b"launched", b"deployed"),
    (b"earned", b"gained"),
    (b"profit", b"earnings"),
    (b"profit", b"revenue"),
    (b"rose", b"increased"),
    (b"fell", b"decreased"),
    (b"dropped", b"decreased"),
    (b"crashed", b"failed"),
    (b"protocol", b"system"),
    (b"protocol", b"contract"),
    (b"tokens", b"coins"),
    (b"address", b"account"),
    (b"amount", b"quantity"),
    (b"raised", b"earned"),
];

fn is_synonym(a: &[u8], b: &[u8]) -> bool {
    let mut i = 0;
    while i < SYNONYM_PAIRS.len() {
        let (x, y) = SYNONYM_PAIRS[i];
        if (eq_ci(a, x) && eq_ci(b, y)) || (eq_ci(a, y) && eq_ci(b, x)) {
            return true;
        }
        i += 1;
    }
    false
}

// Check if a word starts with a common negative prefix
// Used to prevent false substring matches like "success" → "unsuccessful"
fn has_negative_prefix(word: &[u8]) -> bool {
    if word.len() >= 4 && word[0] == b'u' && word[1] == b'n' { return true; }
    if word.len() >= 6 && word[0] == b'd' && word[1] == b'i' && word[2] == b's' { return true; }
    if word.len() >= 6 && word[0] == b'n' && word[1] == b'o' && word[2] == b'n' { return true; }
    if word.len() >= 6 && word[0] == b'm' && word[1] == b'i' && word[2] == b's' { return true; }
    false
}

fn tokens_equivalent(a: &[u8], b: &[u8]) -> bool {
    // 1. Exact case-insensitive match
    if eq_ci(a, b) { return true; }

    // 2. Canonicalized fact match (numbers, hex, ordinals, currency)
    if is_fact_token(a) || is_fact_token(b) {
        let mut buf_a = [0u8; NUM_BUF_LEN];
        let mut buf_b = [0u8; NUM_BUF_LEN];
        let ca = canonicalize_fact(a, &mut buf_a);
        let cb = canonicalize_fact(b, &mut buf_b);
        if eq_ci(ca, cb) { return true; }
    }

    // 3. Synonym table match
    if is_synonym(a, b) { return true; }

    // 4. Guarded substring match
    // - Both tokens must be >= 4 chars
    // - Shorter must be >= 60% of longer's length
    // - Longer must NOT start with a negative prefix (un-, dis-, non-, mis-)
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    if shorter.len() >= 4
        && shorter.len() * 10 >= longer.len() * 6
        && !has_negative_prefix(longer)
        && is_substring(shorter, longer)
    {
        return true;
    }

    false
}

// ========== 5. SCORING ENGINE ==========
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

    // --- GATE 1: NET ASSERTION POLARITY ---
    let gt_pol = compute_net_polarity(&gt_tokens);
    let ma_pol = compute_net_polarity(&ma_tokens);

    if gt_pol != 0 && ma_pol != 0 && gt_pol != ma_pol {
        return 0.0;
    }

    // --- GATE 1.5: ANTONYM PAIR CONTRADICTION ---
    // Catches cases like profit→loss, earned→lost where net-polarity can't detect
    if has_antonym_contradiction(&gt_tokens, &ma_tokens) {
        return 0.0;
    }

    // --- GATE 2: CANONICAL FACT INVARIANT ---
    if !facts_all_satisfied(&gt_tokens, &ma_tokens) {
        return 0.0;
    }

    // --- CONTENT WORD RECALL ---
    let mut gt_content_count = 0usize;
    let mut matched_content = 0usize;

    let mut i = 0;
    while i < gt_tokens.count {
        let gt_w = gt_tokens.get(i);
        if is_stopword(gt_w) { i += 1; continue; }
        gt_content_count += 1;

        let mut matched = false;
        let mut j = 0;
        while j < ma_tokens.count {
            let ma_w = ma_tokens.get(j);
            if tokens_equivalent(gt_w, ma_w) {
                matched = true;
                break;
            }
            j += 1;
        }
        if matched {
            matched_content += 1;
        }
        i += 1;
    }

    let raw_recall = if gt_content_count == 0 {
        1.0
    } else {
        matched_content as f32 / gt_content_count as f32
    };

    // --- QUADRATIC VERBOSITY DAMPENING ---
    let verbosity_ratio = ma_tokens.count as f32 / gt_tokens.count as f32;
    let dampened_recall = if verbosity_ratio > 2.0 {
        let factor = 2.0 / verbosity_ratio;
        raw_recall * factor * factor
    } else {
        raw_recall
    };

    // --- SATURATION SCORING CURVE v10 ---
    // Good zone (dampened_recall >= 0.35): maps to [0.99, 1.00]
    // Bad zone  (dampened_recall <  0.35): quartic crush to [0.000, 0.001]
    let score_val = if dampened_recall >= 0.35 {
        0.99 + 0.01 * ((dampened_recall - 0.35) / 0.65)
    } else {
        let norm = dampened_recall / 0.35;
        0.001 * norm * norm * norm * norm
    };

    if score_val < 0.0 { 0.0 }
    else if score_val > 1.0 { 1.0 }
    else { score_val }
}

// ========== WASM INTERFACE ==========
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
