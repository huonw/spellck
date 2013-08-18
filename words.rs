use std::str;

/// Struct for the `subwords` iterator.
pub struct SubwordIter<'self> {
    priv s: &'self str,
    priv iter: str::CharOffsetIterator<'self>,
    priv word_start: uint,
}

/// Iterate over the "subwords" of a string, e.g. `FooBar` -> `Foo`,
/// `Bar`; `foo_bar` -> `foo`, `bar`; `AB Cd123e` -> `A`, `B`, `Cd`,
/// `e`.
pub fn subwords<'a>(s: &'a str) -> SubwordIter<'a> {
    SubwordIter {
        s: s,
        iter: s.char_offset_iter(),
        word_start: -1u
    }
}

impl<'self> Iterator<&'self str> for SubwordIter<'self> {
    fn next(&mut self) -> Option<&'self str> {
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

    assert_eq!(words(s).to_owned_vec(),
               ~["Foo", "bar", "Baz", "Äåö", "A", "B", "C"]);
}
