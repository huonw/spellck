use std::hashmap;
use std::ascii::StrAsciiExt;

use syntax::visit;
use syntax::visit::Visitor;
use syntax::parse::token;
use syntax::codemap::span;
use syntax::ast::*;
use syntax::attr::AttrMetaMethods;

use words;

/// Keeps track of the reference dictionary and the misspelled words
/// through a traversal of the whole ast.
pub struct SpellingVisitor {
    words: hashmap::HashSet<~str>,
    misspellings: hashmap::HashMap<span, hashmap::HashSet<~str>>,
    doc_only: bool
}

impl SpellingVisitor {
    pub fn new(words: hashmap::HashSet<~str>) -> SpellingVisitor {
        SpellingVisitor {
            words: words,
            misspellings: hashmap::HashMap::new(),
            doc_only: false
        }
    }

    pub fn clear(&mut self) {
        self.misspellings.clear()
    }

    /// Checks if the given string is a correct "word", without
    /// splitting it at all. Any word that isn't entirely alphabetic
    /// is automatically considered a proper word.
    fn raw_word_is_correct(&mut self, w: &str) -> bool {
        self.words.contains_equiv(&w) ||
            !w.iter().all(|c| c.is_alphabetic()) ||
            self.words.contains_equiv(&w.to_ascii_lower())
    }

    /// Checks the given word for correctness, including splitting
    /// `foo_bar` and `FooBar` into `foo` & `bar` and `Foo` & `Bar`
    /// respectively. This inserts any incorrect word(s) into the
    /// misspelling map.
    fn check_subwords(&mut self, w: &str, sp: span) {
        for w in words::words(w) {
            if !self.raw_word_is_correct(w) {
                let insert = |_: &span, _: ()| {
                    let mut set = hashmap::HashSet::new();
                    set.insert(w.to_owned());
                    set
                };
                self.misspellings.mangle(sp, (),
                                         insert,
                                         |_, set, _| { set.insert(w.to_owned()); });
            }
        }
    }

    /// Checks a single ident
    fn check_ident(&mut self, id: ident, sp: span) {
        if self.doc_only { return }

        // spooky action at a distance; extracts the string
        // representation from TLS.
        let word = token::ident_to_str(&id);
        // secret rust internals, e.g. __std_macros
        if word.starts_with("__") { return }

        // the ident itself is correct, so shortcircuit to avoid doing
        // any of the submatching done below.
        if self.raw_word_is_correct(word) {
            return
        }

        self.check_subwords(word, sp);
    }

    /// Check the #[doc=""] (and the commment forms) attributes for
    /// spelling.
    fn check_doc_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs.iter() {
            match attr.name_str_pair() {
                Some((doc, doc_str)) if "doc" == doc => {
                    self.check_subwords(doc_str, attr.span);
                }
                _ => {}
            }
        }
    }

    /// Spellcheck the whole crate.
    pub fn check_crate(@mut self, crate: &Crate) {
        self.check_doc_attrs(crate.attrs);
        visit::visit_crate(self as @mut Visitor<()>, crate, ())
    }
}

// visits anything that could be visible to the outside world,
// e.g. documentation, pub fns, pub mods etc and checks their
// spelling.
impl Visitor<()> for SpellingVisitor {
    fn visit_mod(@mut self,
                 module: &_mod,
                 _span: span,
                 _node_id: NodeId,
                 env: ()) {
        visit::visit_mod(self as @mut Visitor<()>, module, env)
    }
    fn visit_view_item(@mut self, view_item: &view_item, _env: ()) {
        // only check the ident for `use self = foo;`; since there's
        // nothing else the user can do to control the name.
        if view_item.vis == public {
            self.check_doc_attrs(view_item.attrs);
            match view_item.node {
                view_item_use(ref vps) => {
                    for &vp in vps.iter() {
                        match vp.node {
                            view_path_simple(id, _, _) => {
                                self.check_ident(id, vp.span);
                            }
                            _ => {}
                        }
                    }
                }
                view_item_extern_mod(*) => {}
            }
        }
    }
    fn visit_foreign_item(@mut self, foreign_item: @foreign_item, _env: ()) {
        // don't check the ident; there's nothing the user can do to
        // control the name.
        if foreign_item.vis != private {
            // (the visibility rules seems to be strange here, pub is
            // just ignored)
            self.check_doc_attrs(foreign_item.attrs);
        }
    }
    fn visit_item(@mut self, item: @item, env: ()) {
        // no need to check the names/docs of private things
        // (although there may be public things inside them that
        // are re-exported somewhere else, so still recur). (Also,
        // all(?) items inherit private visibility.)
        let should_check_doc = item.vis == public || match item.node {
            item_impl(*) => true,
            _ => false
        };

        if item.vis == public {
            self.check_ident(item.ident, item.span);
        }
        if should_check_doc {
            self.check_doc_attrs(item.attrs);
        }

        match item.node {
            // no visitor method for enum variants so have to do it by
            // hand. This is probably (subtly or otherwise) incorrect
            // wrt to visibility.
            item_enum(ref ed, _) => {
                for var in ed.variants.iter() {
                    let no_check = var.node.vis == private ||
                        (var.node.vis == inherited && item.vis != public);

                    if !no_check {
                        self.check_ident(var.node.name, var.span);
                        self.check_doc_attrs(var.node.attrs);
                    }
                }
            }
            item_mod(*) | item_foreign_mod(*) | item_struct(*) => {
                visit::visit_item(self as @mut Visitor<()>, item, env)
            }
            // impl Type { ... }
            item_impl(_, None, _, ref methods) => {
                for &method in methods.iter() {
                    if method.vis == public {
                        self.check_ident(method.ident, method.span);
                        self.check_doc_attrs(method.attrs);
                    }
                }
            }
            // impl Trait for Type { ... }, only check the docs, the
            // method names come from elsewhere.
            item_impl(_, Some(*), _, _) => {
                let old_d_o = self.doc_only;
                self.doc_only = true;
                visit::visit_item(self as @mut Visitor<()>, item, env);
                self.doc_only = old_d_o;
            }
            item_trait(*) if item.vis == public => {
                visit::visit_item(self as @mut Visitor<()>, item, env)
            }
            _ => {}
        }
    }

    fn visit_ty_method(@mut self, method_type: &TypeMethod, env: ()) {
        self.check_doc_attrs(method_type.attrs);
        self.check_ident(method_type.ident, method_type.span);
        visit::visit_ty_method(self as @mut Visitor<()>, method_type, env)
    }
    fn visit_trait_method(@mut self, trait_method: &trait_method, env: ()) {
        match *trait_method {
            required(_) => {
                visit::visit_trait_method(self as @mut Visitor<()>, trait_method, env)
            }
            provided(method) => {
                self.check_doc_attrs(method.attrs);
                self.check_ident(method.ident, method.span);
            }
        }
    }

    fn visit_struct_def(@mut self,
                        struct_definition: @struct_def,
                        identifier: ident,
                        generics: &Generics,
                        node_id: NodeId,
                        env: ()) {
        visit::visit_struct_def(self as @mut Visitor<()>,
                                struct_definition,
                                identifier,
                                generics,
                                node_id,
                                env)
    }
    fn visit_struct_field(@mut self, struct_field: @struct_field, _env: ()) {
        match struct_field.node.kind {
            named_field(id, vis) => {
                match vis {
                    public | inherited => {
                        self.check_ident(id, struct_field.span);
                        self.check_doc_attrs(struct_field.node.attrs);
                    }
                    private => {}
                }
            }
            unnamed_field => {}
        }

        // no need to recur; nothing below this level to check.
    }

    /// we're only interested in top-level things, so we can just
    /// ignore these entirely.
    fn visit_local(@mut self, _local: @Local, _env: ()) {}
    fn visit_block(@mut self, _block: &Block, _env: ()) {}
    fn visit_stmt(@mut self, _statement: @stmt, _env: ()) {}
    fn visit_arm(@mut self, _arm: &arm, _env: ()) {}
    fn visit_pat(@mut self, _pattern: @pat, _env: ()) {}
    fn visit_decl(@mut self, _declaration: @decl, _env: ()) {}
    fn visit_expr(@mut self, _expression: @expr, _env: ()) {}
    fn visit_expr_post(@mut self, _expression: @expr, _: ()) {}
    fn visit_ty(@mut self, _typ: &Ty, _env: ()) {}
    fn visit_generics(@mut self, _generics: &Generics, _env: ()) {}
    fn visit_fn(@mut self,
                _function_kind: &visit::fn_kind,
                _function_declaration: &fn_decl,
                _block: &Block,
                _span: span,
                _node_id: NodeId,
                _env: ()) {}
}
