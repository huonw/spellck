//! Utilities for iterating over the "words" in a string.

use std::str;

/// Struct for the `subwords` iterator.
pub struct SubwordIter<'a> {
    priv s: &'a str,
    priv iter: str::CharOffsets<'a>,
    priv word_start: uint,
}

/// Iterate over the "subwords" of a string, e.g. `FooBar` -> `Foo`,
/// `Bar`; `foo_bar` -> `foo`, `bar`; `AB Cd123e` -> `A`, `B`, `Cd`,
/// `e`.
pub fn subwords<'a>(s: &'a str) -> SubwordIter<'a> {
    SubwordIter {
        s: s,
        iter: s.char_indices(),
        word_start: -1u
    }
}

impl<'a> Iterator<&'a str> for SubwordIter<'a> {
    fn next(&mut self) -> Option<&'a str> {
        let mut word_start = self.word_start;
        for (offset, c) in self.iter {
            // skip leading non-alphabetic characters
            let alpha = c.is_alphabetic();
            if word_start == -1 {
                if alpha {
                    word_start = offset
                }
            } else {
                if !alpha || c.is_uppercase() {
                    self.word_start = if alpha {
                        // need to reuse this character for the next word
                        offset
                    } else {
                        -1
                    };

                    return Some(self.s.slice(word_start, offset))
                }
            }
        }
        if word_start == -1 {
            None
        } else {
            self.word_start = -1;
            Some(self.s.slice_from(word_start))
        }
    }
}

#[test]
fn test_words() {
    let s = "Foo_barBazÄåöAB123C";

    assert_eq!(subwords(s).collect::<Vec<_>>(),
               vec!("Foo", "bar", "Baz", "Äåö", "A", "B", "C"));
}
