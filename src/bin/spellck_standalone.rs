#![crate_name = "spellck_standalone"]
#![deny(missing_docs)]

//! Prints the misspelled words in the public documentation &
//! identifiers of a crate.

extern crate getopts;
extern crate arena;
extern crate syntax;
extern crate rustc;
extern crate rustc_driver;
extern crate rustc_trans;

extern crate spellck;

use std::{io, os};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, BinaryHeap};
use arena::TypedArena;
use syntax::{ast, ast_map};
use syntax::codemap::{Span, BytePos};
use rustc::middle::{privacy, ty};
use rustc::session::search_paths::SearchPaths;
use rustc::session::{self, config};
use rustc_driver::driver;

use spellck::visitor::SpellingVisitor;

static DEFAULT_DICT: &'static str = "/usr/share/dict/words";
static LIBDIR: &'static str = "/usr/local/lib/rustlib/x86_64-unknown-linux-gnu/lib";

fn main() {
    let args = os::args();
    let opts = &[getopts::optmulti("d", "dict",
                                  "dictionary file (a list of words, one per line)", "PATH"),
                getopts::optflag("n", "no-def-dict", "don't use the default dictionary"),
                getopts::optflag("h", "help", "show this help message")];

    let matches = getopts::getopts(args.tail(), opts).unwrap();
    if matches.opt_present("help") {
        println!("{}", getopts::usage(args[0].as_slice(), opts));
        return;
    }

    let mut words = HashSet::new();

    if !matches.opt_present("no-def-dict") {
        if !read_lines_into(&Path::new(DEFAULT_DICT), &mut words) {
            return
        }
    }
    for dict in matches.opt_strs("d").into_iter().chain(matches.opt_strs("dict").into_iter()) {
        if !read_lines_into(&Path::new(dict), &mut words) {
            return
        }
    }

    let mut search_paths = SearchPaths::new();
    search_paths.add_path(LIBDIR);
    let externs = HashMap::new();

    // one visitor; the internal list of misspelled words gets reset
    // for each file, since the spans could conflict.
    let any_mistakes = Cell::new(false);

    for name in matches.free.iter() {
        get_ast(Path::new(name), search_paths.clone(), externs.clone(),
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
                let word_vec: Vec<&str> = words.iter().map(|s| &s[]).collect();

                println!("{}: misspelled {words}: {}",
                         sp_text,
                         word_vec.connect(", "),
                         words = if words.len() == 1 {"word"} else {"words"});

                // first line; no lines = no printing
                match &lines.lines[] {
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
        os::set_exit_status(1)
    }
}

/// Load each line of the file `p` into the given `Extend` object.
fn read_lines_into<E: Extend<String>>
                  (p: &Path, e: &mut E) -> bool {
    match io::File::open(p) {
        Ok(mut r) => {
            let s = String::from_utf8(r.read_to_end().unwrap())
                .ok().expect(&format!("{} is not UTF-8", p.display())[]);
            e.extend(s.lines().map(|ss| ss.to_string()));
            true
        }
        Err(e) => {
            let mut stderr = io::stderr();
            (write!(&mut stderr as &mut Writer,
                    "Error reading {}: {}", p.display(), e)).unwrap();
            os::set_exit_status(10);
            false
        }
    }
}

type Externs = HashMap<String, Vec<String>>;

/// Extract the expanded ast of a crate, along with the codemap which
/// connects source code locations to the actual code.
/// Extract the expanded ast of a krate, along with the codemap which
/// connects source code locations to the actual code.
fn get_ast<F>(path: Path,
              search_paths: SearchPaths, externs: Externs,
              f: F)
        where F: Fn(&session::Session, &ast::Crate,
                    &privacy::ExportedItems, &privacy::PublicItems) {
    use syntax::diagnostic;
    use rustc_trans::back::link;

    // cargo culted from rustdoc_ng :(
    let input = config::Input::File(path);

    let sessopts = config::Options {
        maybe_sysroot: Some(os::self_exe_path().unwrap().dir_path()),
        externs: externs,
        search_paths: search_paths,
        .. config::basic_options().clone()
    };

    let codemap = syntax::codemap::CodeMap::new();
    let diagnostic_handler =
        diagnostic::default_handler(diagnostic::Auto, None);
    let span_diagnostic_handler =
        diagnostic::mk_span_handler(diagnostic_handler, codemap);

    let sess = session::build_session_(sessopts, None, span_diagnostic_handler);

    let cfg = config::build_configuration(&sess);


    let mut controller = driver::CompileController::basic();
    controller.after_analysis = driver::PhaseController {
        stop: true,
        callback: Box::new(|state| {
            let ca = state.analysis.unwrap();
            let ty::CrateAnalysis { ref exported_items, ref public_items, ref ty_cx, .. } = *ca;
            f(&ty_cx.sess, ty_cx.map.krate(), exported_items, public_items)
        })
    };

    driver::compile_input(sess, cfg, &input, &None, &None, None, controller);
}
