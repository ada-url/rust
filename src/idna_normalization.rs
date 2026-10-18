//! Dependency-free Unicode NFC normalization over Ada's compact tables.

use alloc::vec::Vec;

use super::tables;

const HANGUL_SBASE: u32 = 0xac00;
const HANGUL_TBASE: u32 = 0x11a7;
const HANGUL_VBASE: u32 = 0x1161;
const HANGUL_LBASE: u32 = 0x1100;
const HANGUL_LCOUNT: u32 = 19;
const HANGUL_VCOUNT: u32 = 21;
const HANGUL_TCOUNT: u32 = 28;
const HANGUL_NCOUNT: u32 = HANGUL_VCOUNT * HANGUL_TCOUNT;
const HANGUL_SCOUNT: u32 = HANGUL_LCOUNT * HANGUL_NCOUNT;

pub(super) fn normalize(input: &mut Vec<u32>) {
    let additional = input
        .iter()
        .map(|code_point| decomposition_length(*code_point).saturating_sub(1))
        .sum();
    if additional != 0 {
        decompose(input, additional);
    }
    reorder_marks(input);
    compose(input);
}

pub(super) fn is_nfc(input: &[u32]) -> bool {
    if input
        .iter()
        .any(|code_point| decomposition_length(*code_point) == 1)
    {
        return false;
    }
    let mut previous = 0;
    for &code_point in input {
        let class = tables::combining_class(code_point);
        if class != 0 && previous > class {
            return false;
        }
        previous = class;
    }
    !would_compose(input)
}

fn decomposition_length(code_point: u32) -> usize {
    if (HANGUL_SBASE..HANGUL_SBASE + HANGUL_SCOUNT).contains(&code_point) {
        return if (code_point - HANGUL_SBASE).is_multiple_of(HANGUL_TCOUNT) {
            2
        } else {
            3
        };
    }
    tables::decomposition(code_point).map_or(0, |bytes| bytes.len() / 4)
}

fn decompose(input: &mut Vec<u32>, additional: usize) {
    let original = input.len();
    input.resize(original + additional, 0);
    let mut destination = input.len();
    for source in (0..original).rev() {
        let code_point = input[source];
        if (HANGUL_SBASE..HANGUL_SBASE + HANGUL_SCOUNT).contains(&code_point) {
            let syllable = code_point - HANGUL_SBASE;
            if !syllable.is_multiple_of(HANGUL_TCOUNT) {
                destination -= 1;
                input[destination] = HANGUL_TBASE + syllable % HANGUL_TCOUNT;
            }
            destination -= 1;
            input[destination] = HANGUL_VBASE + (syllable % HANGUL_NCOUNT) / HANGUL_TCOUNT;
            destination -= 1;
            input[destination] = HANGUL_LBASE + syllable / HANGUL_NCOUNT;
        } else if let Some(decomposition) = tables::decomposition(code_point) {
            for index in (0..decomposition.len() / 4).rev() {
                destination -= 1;
                input[destination] = tables::decomposition_code_point(decomposition, index);
            }
        } else {
            destination -= 1;
            input[destination] = code_point;
        }
    }
}

fn reorder_marks(input: &mut [u32]) {
    for index in 1..input.len() {
        let class = tables::combining_class(input[index]);
        if class == 0 {
            continue;
        }
        let code_point = input[index];
        let mut destination = index;
        while destination != 0 && tables::combining_class(input[destination - 1]) > class {
            input[destination] = input[destination - 1];
            destination -= 1;
        }
        input[destination] = code_point;
    }
}

fn would_compose(input: &[u32]) -> bool {
    let mut index = 0;
    while index < input.len() {
        let current = input[index];
        if (HANGUL_LBASE..HANGUL_LBASE + HANGUL_LCOUNT).contains(&current) {
            if input
                .get(index + 1)
                .is_some_and(|next| (HANGUL_VBASE..HANGUL_VBASE + HANGUL_VCOUNT).contains(next))
            {
                return true;
            }
        } else if (HANGUL_SBASE..HANGUL_SBASE + HANGUL_SCOUNT).contains(&current) {
            if (current - HANGUL_SBASE).is_multiple_of(HANGUL_TCOUNT)
                && input.get(index + 1).is_some_and(|next| {
                    (HANGUL_TBASE + 1..HANGUL_TBASE + HANGUL_TCOUNT).contains(next)
                })
            {
                return true;
            }
        } else {
            let mut previous_class = -1_i16;
            let mut next = index + 1;
            while next < input.len() {
                let class = tables::combining_class(input[next]);
                if previous_class < i16::from(class)
                    && tables::compose(current, input[next]).is_some()
                {
                    return true;
                }
                if class == 0 {
                    break;
                }
                previous_class = i16::from(class);
                next += 1;
            }
        }
        index += 1;
    }
    false
}

fn compose(input: &mut Vec<u32>) {
    let mut source = 0;
    let mut destination = 0;
    while source < input.len() {
        let current = input[source];
        input[destination] = current;
        if (HANGUL_LBASE..HANGUL_LBASE + HANGUL_LCOUNT).contains(&current)
            && input
                .get(source + 1)
                .is_some_and(|next| (HANGUL_VBASE..HANGUL_VBASE + HANGUL_VCOUNT).contains(next))
        {
            source += 1;
            input[destination] = HANGUL_SBASE
                + ((current - HANGUL_LBASE) * HANGUL_VCOUNT + input[source] - HANGUL_VBASE)
                    * HANGUL_TCOUNT;
            if input
                .get(source + 1)
                .is_some_and(|next| (HANGUL_TBASE + 1..HANGUL_TBASE + HANGUL_TCOUNT).contains(next))
            {
                source += 1;
                input[destination] += input[source] - HANGUL_TBASE;
            }
        } else if (HANGUL_SBASE..HANGUL_SBASE + HANGUL_SCOUNT).contains(&current)
            && (current - HANGUL_SBASE).is_multiple_of(HANGUL_TCOUNT)
            && input
                .get(source + 1)
                .is_some_and(|next| (HANGUL_TBASE + 1..HANGUL_TBASE + HANGUL_TCOUNT).contains(next))
        {
            source += 1;
            input[destination] += input[source] - HANGUL_TBASE;
        } else {
            let starter = destination;
            let mut previous_class = -1_i16;
            while source + 1 < input.len() {
                let next = input[source + 1];
                let class = tables::combining_class(next);
                if previous_class < i16::from(class)
                    && let Some(composed) = tables::compose(input[starter], next)
                {
                    input[starter] = composed;
                    source += 1;
                    continue;
                }
                if class == 0 {
                    break;
                }
                previous_class = i16::from(class);
                destination += 1;
                source += 1;
                input[destination] = next;
            }
        }
        source += 1;
        destination += 1;
    }
    input.truncate(destination);
}
