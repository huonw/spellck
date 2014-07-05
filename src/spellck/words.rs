//! Utilities for iterating over the "words" in a string.

use std::str;

/// Struct for the `subwords` iterator.
pub struct SubwordIter<'a> {
    s: &'a str,
    iter: str::CharOffsets<'a>,
    word_start: Option<uint>,
}

/// Iterate over the "subwords" of a string, e.g. `Foobar` -> `Foo`,
/// `Bar`; `foo_bar` -> `foo`, `bar`; `AB Cd123e` -> `A`, `B`, `Cd`,
/// `e`.
pub fn subwords<'a>(s: &'a str) -> SubwordIter<'a> {
    SubwordIter {
        s: s,
        iter: s.char_indices(),
        word_start: None
    }
}

impl<'a> Iterator<&'a str> for SubwordIter<'a> {
    fn next(&mut self) -> Option<&'a str> {
        let mut word_start = self.word_start;
        for (offset, c) in self.iter {
            // skip leading non-alphabetic characters
            let alpha = c.is_alphabetic();
            match word_start {
                None if alpha => word_start = Some(offset),
                None => {}
                Some(ws) if !alpha || c.is_uppercase() => {
                    self.word_start = if alpha {
                        // need to reuse this character for the next word
                        Some(offset)
                    } else {
                        None
                    };

                    return Some(self.s.slice(ws, offset))
                }
                Some(_) => {}
            }
        }
        word_start.map(|ws| { self.word_start = None; self.s.slice_from(ws) })
    }
}

#[test]
fn test_words() {
    let s = "Foo_barBazÄåöAB123C";

    assert_eq!(subwords(s).collect::<Vec<_>>(),
               vec!("Foo", "bar", "Baz", "Äåö", "A", "B", "C"));
}
