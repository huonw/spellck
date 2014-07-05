use std::{os, result};
use std::io::{File, BufferedReader};
use std::collections::HashSet ;

use syntax::ast;

use rustc::lint::{Context, LintArray, LintPass};

use visitor;

static DICT_ENV_VAR: &'static str = "SPELLCK_LINT_DICT";

declare_lint! { MISSPELLINGS, Warn, "detects words that are spelled incorrectly" }

pub struct Misspellings {
    words: HashSet<String>,
    loading_error: Option<String>
}

impl Misspellings {
    pub fn load() -> Misspellings {
        let mut ret = Misspellings {
            words: HashSet::new(),
            loading_error: None
        };
        let paths = match os::getenv(DICT_ENV_VAR) {
            Some(p) => os::split_paths(p),
            None => {
                ret.loading_error = Some(format!("environment variable `{}` not specified",
                                                 DICT_ENV_VAR));
                return ret
            }
        };

        for p in paths.move_iter().map(Path::new) {
            let words = File::open(&p)
                .and_then(|f| {
                    let mut rdr = BufferedReader::new(f);
                    let lines = rdr.lines().map(|l| l.map(|s| s.as_slice().trim().to_string()));
                    result::collect::<_, _, _, Vec<String>>(lines)
                });

            match words {
                Ok(w) => ret.words.extend(w.move_iter()),
                Err(e) => {
                    ret.loading_error = Some(format!("error loading `{}`: {}", p.display(), e));
                    return ret
                }
            }
        }

        ret
    }
}

impl LintPass for Misspellings {
    fn get_lints(&self) -> LintArray {
        lint_array!(MISSPELLINGS)
    }

    fn check_crate(&mut self, cx: &Context, krate: &ast::Crate) {
        let sess = cx.sess();
        match self.loading_error {
            None => {}
            Some(ref e) => {
                sess.err(format!("failed to start misspelling lint: {}", *e).as_slice());
                return
            }
        }
        let mut v = visitor::SpellingVisitor::new(&self.words, cx.exported_items);
        v.check_crate(krate);

        for (&pos, words) in v.misspellings.iter() {
            sess.add_lint(MISSPELLINGS, pos.id, pos.span,
                          format!("misspelled word{}: {}",
                                  if words.len() == 1 { "" } else { "s" },
                                  words.connect(", ")))
        }
    }
}
