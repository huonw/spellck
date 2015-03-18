use std::os;
use std::old_io::{File, BufferedReader};
use std::collections::HashSet ;
use std::borrow::ToOwned;

use syntax::attr;
use syntax::ast;
use syntax::ast::Lit_::{LitStr};
use syntax::ast::MetaItem_::{MetaNameValue};

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

        for p in paths.into_iter().map(Path::new) {
            let words = File::open(&p)
                .and_then(|f| {
                    let mut rdr = BufferedReader::new(f);
                    let lines = rdr.lines().map(|l| l.map(|s| s.trim().to_string()));
                    lines.collect::<Result<Vec<String>, _>>()
                });

            match words {
                Ok(w) => ret.words.extend(w.into_iter()),
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
                sess.err(&format!("failed to start misspelling lint: {}", *e));
                return
            }
        }

        for attribute in krate.attrs.iter() {
            if let MetaNameValue(ref name, ref lit) = attribute.node.value.node {
                if &**name == "spellck_extra_words" {
                    attr::mark_used(attribute);
                    if let LitStr(ref raw_words, _) = lit.node {
                        self.words.extend(raw_words.words().map(|w| w.to_owned()));
                    } else {
                        cx.sess().span_err(attribute.span, "malformed `spellck_extra_words` attribute")
                    }
                }
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
