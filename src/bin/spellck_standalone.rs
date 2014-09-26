#![crate_name = "spellck_standalone"]
#![deny(missing_doc)]
#![feature(managed_boxes)]

//! Prints the misspelled words in the public documentation &
//! identifiers of a crate.

extern crate getopts;
extern crate arena;
extern crate syntax;
extern crate rustc;

extern crate spellck;

use std::{io, os};
use std::collections::{HashSet, PriorityQueue};
use arena::TypedArena;
use syntax::{ast, ast_map};
use syntax::codemap::{Span, BytePos};
use rustc::driver::{driver, session, config};
use rustc::middle::privacy;

use spellck::visitor::SpellingVisitor;

static DEFAULT_DICT: &'static str = "/usr/share/dict/words";
static LIBDIR: &'static str = "/usr/local/lib/rustlib/x86_64-unknown-linux-gnu/lib";

fn main() {
    let args = os::args();
    let opts = [getopts::optmulti("d", "dict",
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

    // one visitor; the internal list of misspelled words gets reset
    // for each file, since the spans could conflict.
    let mut any_mistakes = false;

    for name in matches.free.iter() {
        get_ast(Path::new(name.as_slice()), |sess, krate, export, _public| {
            let cm = sess.codemap();

            let mut visitor = SpellingVisitor::new(&words, &export);
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
            let pq: PriorityQueue<Sort> =
                visitor.misspellings.iter().map(|(pos, v)| Sort { sp: pos.span, words: v }).collect();

            // run through the spans, printing the words that are
            // apparently misspelled
            for Sort {sp, words} in pq.into_sorted_vec().into_iter() {
                any_mistakes = true;

                let lines = cm.span_to_lines(sp);
                let sp_text = cm.span_to_string(sp);

                // [] required for connect :(
                let word_vec: Vec<&str> = words.iter().map(|s| s.as_slice()).collect();

                println!("{}: misspelled {words}: {}",
                         sp_text,
                         word_vec.connect(", "),
                         words = if words.len() == 1 {"word"} else {"words"});

                // first line; no lines = no printing
                match lines.lines.as_slice() {
                    [line_num, ..] => {
                        let line = lines.file.get_line(line_num as int);
                        println!("{}: {}", sp_text, line);
                    }
                    _ => {}
                }
            }
        })
    }

    if any_mistakes {
        os::set_exit_status(1)
    }
}

/// Load each line of the file `p` into the given `Extendable` object.
fn read_lines_into<E: Extendable<String>>
                  (p: &Path, e: &mut E) -> bool {
    match io::File::open(p) {
        Ok(mut r) => {
            let s = String::from_utf8(r.read_to_end().unwrap())
                .ok().expect(format!("{} is not UTF-8", p.display()).as_slice());
            e.extend(s.as_slice().lines().map(|ss| ss.to_string()));
            true
        }
        Err(e) => {
            (write!(&mut io::stderr() as &mut Writer,
                    "Error reading {}: {}", p.display(), e)).unwrap();
            os::set_exit_status(10);
            false
        }
    }
}

/// Extract the expanded ast of a crate, along with the codemap which
/// connects source code locations to the actual code.
/// Extract the expanded ast of a krate, along with the codemap which
/// connects source code locations to the actual code.
fn get_ast<T>(path: Path,
              f: |session::Session, &ast::Crate,
                  privacy::ExportedItems, privacy::PublicItems| -> T) -> T {
    use syntax::diagnostic;
    use rustc::back::link;

    // cargo culted from rustdoc_ng :(
    let input = driver::FileInput(path);

    let sessopts = config::Options {
        maybe_sysroot: Some(os::self_exe_path().unwrap().dir_path()),
        addl_lib_search_paths: std::cell::RefCell::new(
            Some(Path::new(LIBDIR)).into_iter().collect()),
        .. config::basic_options().clone()
    };

    let codemap = syntax::codemap::CodeMap::new();
    let diagnostic_handler =
        diagnostic::default_handler(diagnostic::Auto, None);
    let span_diagnostic_handler =
        diagnostic::mk_span_handler(diagnostic_handler, codemap);

    let sess = session::build_session_(sessopts, None, span_diagnostic_handler);

    let cfg = config::build_configuration(&sess);

    let krate = driver::phase_1_parse_input(&sess, cfg, &input);
    let id = link::find_crate_name(Some(&sess), krate.attrs.as_slice(),
                                   &input);
    let krate = driver::phase_2_configure_and_expand(
        &sess, krate, id.as_slice(), None).unwrap();
    let mut forest = ast_map::Forest::new(krate);
    let ast_map = driver::assign_node_ids_and_map(&sess, &mut forest);
    let type_arena = TypedArena::new();
    let res = driver::phase_3_run_analysis_passes(sess, ast_map, &type_arena, id);
    let driver::CrateAnalysis {
        exported_items, public_items, ty_cx, .. } = res;
    f(ty_cx.sess, ty_cx.map.krate(), exported_items, public_items)
}
