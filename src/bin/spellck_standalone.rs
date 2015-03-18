#![crate_name = "spellck_standalone"]
#![deny(missing_docs)]
#![feature(rustc_private)]
#![feature(collections, exit_status, std_misc)]

//! Prints the misspelled words in the public documentation &
//! identifiers of a crate.

extern crate getopts;
extern crate arena;
extern crate syntax;
extern crate rustc;
extern crate rustc_driver;
extern crate rustc_trans;

#[allow(plugin_as_library)]
extern crate spellck;

use std::env;
use std::path::{AsPath, PathBuf};
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, BinaryHeap};
use syntax::ast;
use syntax::codemap::{Span, BytePos};
use syntax::diagnostics;
use rustc::middle::{privacy, ty};
use rustc::session::{self, config};
use rustc_driver::{driver, pretty, Compilation};

use spellck::visitor::SpellingVisitor;

static DEFAULT_DICT: &'static str = "/usr/share/dict/words";
static LIBDIR: &'static str = "/usr/local/lib/rustlib/x86_64-unknown-linux-gnu/lib";

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let opts = &[getopts::optmulti("d", "dict",
                                  "dictionary file (a list of words, one per line)", "PATH"),
                getopts::optflag("n", "no-def-dict", "don't use the default dictionary"),
                getopts::optflag("h", "help", "show this help message")];

    let matches = getopts::getopts(args.tail(), opts).unwrap();
    if matches.opt_present("help") {
        println!("{}", getopts::usage(&args[0], opts));
        return;
    }

    let mut words = HashSet::new();

    if !matches.opt_present("no-def-dict") {
        if !read_lines_into(DEFAULT_DICT, &mut words) {
            return
        }
    }
    for dict in matches.opt_strs("d").into_iter().chain(matches.opt_strs("dict").into_iter()) {
        if !read_lines_into(&dict, &mut words) {
            return
        }
    }

    // one visitor; the internal list of misspelled words gets reset
    // for each file, since the spans could conflict.
    let any_mistakes = Cell::new(false);

    for name in matches.free {
        get_ast(name,
                |sess, krate, export, _public| {
            let cm = sess.codemap();

            let mut visitor = SpellingVisitor::new(&words, export);
            visitor.check_crate(krate);

            struct Sort<'a> {
                sp: Span,
                words: &'a Vec<String>
            }
            impl<'a> PartialEq for Sort<'a> {
                fn eq(&self, other: &Sort<'a>) -> bool {
                    self.sp == other.sp
                }
            }
            impl<'a> PartialOrd for Sort<'a> {
                fn partial_cmp(&self, other: &Sort<'a>) -> Option<Ordering> {
                    Some(self.cmp(other))
                }
            }
            impl<'a> Eq for Sort<'a> {}
            impl<'a> Ord for Sort<'a> {
                fn cmp(&self, other: &Sort<'a>) -> Ordering {
                    let Span { lo: BytePos(slo), hi: BytePos(shi), .. } = self.sp;
                    let Span { lo: BytePos(olo), hi: BytePos(ohi), .. } = other.sp;
                    (slo, shi).cmp(&(olo, ohi))
                }
            }

            // extract the lines in order of the spans, so that e.g. files
            // are grouped together, and lines occur in increasing order.
            let pq: BinaryHeap<Sort> =
                visitor.misspellings.iter().map(|(pos, v)| Sort { sp: pos.span, words: v }).collect();

            // run through the spans, printing the words that are
            // apparently misspelled
            for Sort {sp, words} in pq.into_sorted_vec().into_iter() {
                any_mistakes.set(true);

                let lines = cm.span_to_lines(sp);
                let sp_text = cm.span_to_string(sp);

                // [] required for connect :(
                let word_vec: Vec<&str> = words.iter().map(|s| &**s).collect();

                println!("{}: misspelled {words}: {}",
                         sp_text,
                         word_vec.connect(", "),
                         words = if words.len() == 1 {"word"} else {"words"});

                // first line; no lines = no printing
                match &*lines.lines {
                    [line_num, ..] => {
                        if let Some(line) = lines.file.get_line(line_num) {
                            println!("{}: {}", sp_text, line);
                        }
                    }
                    _ => {}
                }
            }
        })
    }

    if any_mistakes.get() {
        env::set_exit_status(1)
    }
}

/// Load each line of the file `p` into the given `Extend` object.
fn read_lines_into<P: AsPath + ::std::fmt::Debug + ?Sized, E: Extend<String>>
                  (p: &P, e: &mut E) -> bool {
    match File::open(p) {
        Ok(mut r) => {
            let mut s = String::new();
            r.read_to_string(&mut s).unwrap();
            e.extend(s.lines().map(|ss| ss.to_string()));
            true
        }
        Err(e) => {
            let mut stderr = io::stderr();
            (write!(&mut stderr,
                    "Error reading {:?}: {}", p, e)).unwrap();
            env::set_exit_status(10);
            false
        }
    }
}

type Externs = HashMap<String, Vec<String>>;

struct Calls<F> {
    f: Option<F>
}

impl<'a, F> rustc_driver::CompilerCalls<'a> for Calls<F>
    where F: 'a + Fn(&session::Session, &ast::Crate,
                     &privacy::ExportedItems, &privacy::PublicItems)
{
    fn early_callback(&mut self,
                      _matches: &getopts::Matches,
                      _descriptions: &diagnostics::registry::Registry)
                      -> Compilation {
        Compilation::Continue
    }

    fn no_input(&mut self,
                _matches: &getopts::Matches,
                _sopts: &config::Options,
                _odir: &Option<PathBuf>,
                _ofile: &Option<PathBuf>,
                _descriptions: &diagnostics::registry::Registry)
                -> Option<(config::Input, Option<PathBuf>)> {
        unreachable!()
    }

    fn parse_pretty(&mut self,
                    _sess: &session::Session,
                    _matches: &getopts::Matches)
                    -> Option<(pretty::PpMode, Option<pretty::UserIdentifiedItem>)> {
        None
    }

    fn late_callback(&mut self,
                     matches: &getopts::Matches,
                     sess: &session::Session,
                     input: &config::Input,
                     odir: &Option<PathBuf>,
                     ofile: &Option<PathBuf>)
                     -> Compilation {
        rustc_driver::RustcDefaultCalls.late_callback(matches, sess, input, odir, ofile)
    }

    fn build_controller(&mut self, _sess: &session::Session) -> driver::CompileController<'a> {
        let f = self.f.take().unwrap();
        let mut controller = driver::CompileController::basic();
        controller.after_analysis = driver::PhaseController {
            stop: rustc_driver::Compilation::Stop,
            callback: Box::new(move |state| {
                let ca = state.analysis.unwrap();
                let ty::CrateAnalysis { ref exported_items, ref public_items, ref ty_cx, .. } = *ca;
                f(&ty_cx.sess, ty_cx.map.krate(), exported_items, public_items)
            })
        };
        controller
    }
}

/// Extract the expanded ast of a krate, along with the codemap which
/// connects source code locations to the actual code.
#[allow(deprecated)]
fn get_ast<F>(path: String,
              f: F)
        where F: Fn(&session::Session, &ast::Crate,
                    &privacy::ExportedItems, &privacy::PublicItems)
{
    let mut calls = Calls { f: Some(f) };
    rustc_driver::run_compiler(&[format!("-L{}", LIBDIR), path], &mut calls);
}
