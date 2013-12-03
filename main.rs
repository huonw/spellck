#[deny(missing_doc)];
#[feature(managed_boxes)];

extern mod extra;
extern mod syntax;
extern mod rustc;
use std::{io, os, str};
use std::hashmap::HashSet;
use extra::priority_queue;
use syntax::{ast, codemap};

pub mod words;
mod visitor;

static DEFAULT_DICT: &'static str = "/usr/share/dict/words";

fn main() {
    use extra::getopts::groups;

    let args = std::os::args();
    let opts = ~[groups::optmulti("d", "dict",
                                  "dictionary file (a list of words, one per line)", "PATH"),
                 groups::optflag("n", "no-def-dict", "don't use the default dictionary"),
                 groups::optflag("h", "help", "show this help message")];

    let matches = groups::getopts(args.tail(), opts).unwrap();
    if matches.opts_present([~"h", ~"help"]) {
        println(groups::usage(args[0], opts));
        return;
    }

    let mut words = HashSet::new();

    if !matches.opts_present([~"n", ~"no-def-dict"]) {
        if !read_lines_into(&Path::init(DEFAULT_DICT), &mut words) {
            return
        }
    }
    for dict in matches.opt_strs("d").move_iter().chain(matches.opt_strs("dict").move_iter()) {
        if !read_lines_into(&Path::init(dict), &mut words) {
            return
        }
    }

    // one visitor; the internal list of misspelled words gets reset
    // for each file, since the spans could conflict.
    let mut any_mistakes = false;

    for name in matches.free.iter() {
        let (cm, crate) = get_ast(Path::init(name.as_slice()));

        let mut visitor = visitor::SpellingVisitor::new(&words);
        visitor.check_crate(&crate);

        struct Sort<'self> {
            sp: codemap::Span,
            words: &'self HashSet<~str>
        }
        impl<'self> Ord for Sort<'self> {
            fn lt(&self, other: &Sort<'self>) -> bool {
                self.sp.lo < other.sp.lo ||
                    (self.sp.lo == other.sp.lo && self.sp.hi < other.sp.hi)
            }
        }

        // extract the lines in order of the spans, so that e.g. files
        // are grouped together, and lines occur in increasing order.
        let pq: priority_queue::PriorityQueue<Sort> =
            visitor.misspellings.iter().map(|(k, v)| Sort { sp: *k, words: v }).collect();

        // run through the spans, printing the words that are
        // apparently misspelled
        for Sort {sp, words} in pq.to_sorted_vec().move_iter() {
            any_mistakes = true;

            let lines = cm.span_to_lines(sp);
            let sp_text = cm.span_to_str(sp);

            // [] required for connect :(
            let word_vec = words.iter().map(|s| s.as_slice()).to_owned_vec();

            println!("{}: misspelled {len, plural, =1{word} other{words}}: {}",
                     sp_text,
                     word_vec.connect(", "),
                     len=words.len());

            // first line; no lines = no printing
            match lines.lines {
                [line_num, ..] => {
                    let line = lines.file.get_line(line_num as int);
                    println!("{}: {}", sp_text, line);
                }
                _ => {}
            }
        }
    }

    if any_mistakes {
        os::set_exit_status(1)
    }
}

/// Load each line of the file `p` into the given `Extendable` object.
fn read_lines_into<E: Extendable<~str>>
                  (p: &Path, e: &mut E) -> bool {
    match io::result(|| io::File::open(p)) {
        Ok(Some(mut r)) => {
            let r = r.read_to_end();
            let s = str::from_utf8_owned(r);
            e.extend(&mut s.lines().map(|ss| ss.to_owned()));
            true
        }
        Ok(None) => fail!("impossible?"),
        Err(e) => {
            write!(&mut io::stderr() as &mut Writer,
                   "Error reading {}: {}", p.display(), e.to_str());
            os::set_exit_status(10);
            false
        }
    }
}

/// Extract the expanded ast of a crate, along with the codemap which
/// connects source code locations to the actual code.
fn get_ast(path: Path) -> (@codemap::CodeMap, ast::Crate) {
    use rustc::driver::{driver, session};
    use syntax::diagnostic;

    // cargo culted from rustdoc_ng :(
    let parsesess = syntax::parse::new_parse_sess(None);
    let input = driver::file_input(path);

    let sessopts = @session::options {
        binary: @"spellck",
        .. (*session::basic_options()).clone()
    };


    let diagnostic_handler = diagnostic::mk_handler(None);
    let span_diagnostic_handler =
        diagnostic::mk_span_handler(diagnostic_handler, parsesess.cm);

    let sess = driver::build_session_(sessopts, parsesess.cm,
                                      @diagnostic::DefaultEmitter as @diagnostic::Emitter,
                                      span_diagnostic_handler);

    let cfg = driver::build_configuration(sess);

    let crate = driver::phase_1_parse_input(sess, cfg.clone(), &input);

    (parsesess.cm,
     driver::phase_2_configure_and_expand(sess, cfg, crate))
}
