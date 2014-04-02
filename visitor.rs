use collections::hashmap;
use std::ascii::StrAsciiExt;

use syntax::{ast, visit};
use syntax::visit::Visitor;
use syntax::parse::token;
use syntax::codemap::Span;
use syntax::attr::AttrMetaMethods;

use words;

/// Keeps track of the reference dictionary and the misspelled words
/// through a traversal of the whole ast.
pub struct SpellingVisitor<'a> {
    /// The reference dictionary.
    words: &'a hashmap::HashSet<~str>,
    /// The misspelled words, indexed by the span on which they occur.
    pub misspellings: hashmap::HashMap<Span, hashmap::HashSet<~str>>,

    /// Whether the traversal should only check documentation, not
    /// idents; gets controlled internally, e.g. for `extern` blocks.
    doc_only: bool
}

impl<'a> SpellingVisitor<'a> {
    /// ast::Create a new Spelling Visitor.
    pub fn new<'a>(words: &'a hashmap::HashSet<~str>) -> SpellingVisitor<'a> {
        SpellingVisitor {
            words: words,
            misspellings: hashmap::HashMap::new(),
            doc_only: false
        }
    }

    /// Checks if the given string is a correct "word", without
    /// splitting it at all. Any word that isn't entirely alphabetic
    /// is automatically considered a proper word.
    fn raw_word_is_correct(&mut self, w: &str) -> bool {
        self.words.contains_equiv(&w) ||
            !w.chars().all(|c| c.is_alphabetic()) ||
            self.words.contains_equiv(&w.to_ascii_lower())
    }

    /// Check a word for correctness, including splitting `foo_bar`
    /// and `FooBar` into `foo` & `bar` and `Foo` & `Bar`
    /// respectively. This inserts any incorrect word(s) into the
    /// misspelling map.
    fn check_subwords(&mut self, w: &str, sp: Span) {
        for w in words::subwords(w) {
            if !self.raw_word_is_correct(w) {
                let set =
                    self.misspellings.find_or_insert_with(sp, |_| hashmap::HashSet::new());
                set.insert(w.to_owned());
            }
        }
    }

    /// Check a single ident for misspellings; possibly separating it
    /// into subwords.
    fn check_ident(&mut self, id: ast::Ident, sp: Span) {
        if self.doc_only { return }

        // spooky action at a distance; extracts the string
        // representation from TLS.
        let word_ = token::get_ident(id);
        let word = word_.get();
        // secret rust internals, e.g. __std_macros
        if word.starts_with("__") { return }

        // the ident itself is correct, so shortcircuit to avoid doing
        // any of the submatching done below.
        if self.raw_word_is_correct(word) {
            return
        }

        self.check_subwords(word, sp);
    }

    /// Check the #[doc="..."] (and the commment forms) attributes for
    /// spelling.
    fn check_doc_attrs(&mut self, attrs: &[ast::Attribute]) {
        for attr in attrs.iter() {
            match attr.name_str_pair() {
                Some((ref doc, ref doc_str)) if "doc" == doc.get() => {
                    self.check_subwords(doc_str.get(), attr.span);
                }
                _ => {}
            }
        }
    }

    /// Spell-check a whole krate.
    pub fn check_crate(&mut self, krate: &ast::Crate) {
        self.check_doc_attrs(krate.attrs.as_slice());
        visit::walk_crate(self, krate, ())
    }
}

// visits anything that could be visible to the outside world,
// e.g. documentation, pub fns, pub mods etc and checks their
// spelling.
impl<'a> Visitor<()> for SpellingVisitor<'a> {
    fn visit_mod(&mut self,
                 module: &ast::Mod,
                 _span: Span,
                 _node_id: ast::NodeId,
                 env: ()) {
        visit::walk_mod(self, module, env)
    }
    fn visit_view_item(&mut self, view_item: &ast::ViewItem, _env: ()) {
        // only check the ident for `use self = foo;`; since there's
        // nothing else the user can do to control the name.
        if view_item.vis == ast::Public {
            self.check_doc_attrs(view_item.attrs.as_slice());
            match view_item.node {
                ast::ViewItemUse(ref vps) => {
                    for &vp in vps.iter() {
                        match vp.node {
                            ast::ViewPathSimple(id, _, _) => {
                                self.check_ident(id, vp.span);
                            }
                            _ => {}
                        }
                    }
                }
                ast::ViewItemExternCrate(..) => {}
            }
        }
    }
    fn visit_foreign_item(&mut self, foreign_item: &ast::ForeignItem, _env: ()) {
        // don't check the ident; there's nothing the user can do to
        // control the name.
        if foreign_item.vis != ast::Private {
            // (the visibility rules seems to be strange here, pub is
            // just ignored)
            self.check_doc_attrs(foreign_item.attrs.as_slice());
        }
    }
    fn visit_item(&mut self, item: &ast::Item, env: ()) {
        // no need to check the names/docs of ast::Private things
        // (although there may be ast::Public things inside them that
        // are re-exported somewhere else, so still recur). (Also,
        // all(?) items inherit ast::Private visibility.)
        let should_check_doc = item.vis == ast::Public || match item.node {
            ast::ItemImpl(..) => true,
            _ => false
        };

        if item.vis == ast::Public {
            self.check_ident(item.ident, item.span);
        }
        if should_check_doc {
            self.check_doc_attrs(item.attrs.as_slice());
        }

        match item.node {
            // no visitor method for enum variants so have to do it by
            // hand. This is probably (subtly or otherwise) incorrect
            // wrt to visibility.
            ast::ItemEnum(ref ed, _) => {
                for var in ed.variants.iter() {
                    let no_check = var.node.vis == ast::Private ||
                        (var.node.vis == ast::Inherited && item.vis != ast::Public);

                    if !no_check {
                        self.check_ident(var.node.name, var.span);
                        self.check_doc_attrs(var.node.attrs.as_slice());
                    }
                }
            }
            ast::ItemMod(..) | ast::ItemForeignMod(..) | ast::ItemStruct(..) => {
                visit::walk_item(self, item, env)
            }
            // impl Type { ... }
            ast::ItemImpl(_, None, _, ref methods) => {
                for &method in methods.iter() {
                    if method.vis == ast::Public {
                        self.check_ident(method.ident, method.span);
                        self.check_doc_attrs(method.attrs.as_slice());
                    }
                }
            }
            // impl Trait for Type { ... }, only check the docs, the
            // method names come from elsewhere.
            ast::ItemImpl(_, Some(..), _, _) => {
                let old_d_o = self.doc_only;
                self.doc_only = true;
                visit::walk_item(self, item, env);
                self.doc_only = old_d_o;
            }
            ast::ItemTrait(..) if item.vis == ast::Public => {
                visit::walk_item(self, item, env)
            }
            _ => {}
        }
    }

    fn visit_ty_method(&mut self, method_type: &ast::TypeMethod, env: ()) {
        self.check_doc_attrs(method_type.attrs.as_slice());
        self.check_ident(method_type.ident, method_type.span);
        visit::walk_ty_method(self, method_type, env)
    }
    fn visit_trait_method(&mut self, trait_method: &ast::TraitMethod, env: ()) {
        match *trait_method {
            ast::Required(_) => {
                visit::walk_trait_method(self, trait_method, env)
            }
            ast::Provided(method) => {
                self.check_doc_attrs(method.attrs.as_slice());
                self.check_ident(method.ident, method.span);
            }
        }
    }

    fn visit_struct_def(&mut self,
                        struct_definition: &ast::StructDef,
                        identifier: ast::Ident,
                        generics: &ast::Generics,
                        node_id: ast::NodeId,
                        env: ()) {
        visit::walk_struct_def(self,
                               struct_definition,
                               identifier,
                               generics,
                               node_id,
                               env)
    }
    fn visit_struct_field(&mut self, struct_field: &ast::StructField, _env: ()) {
        match struct_field.node.kind {
            ast::NamedField(id, vis) => {
                match vis {
                    ast::Public | ast::Inherited => {
                        self.check_ident(id, struct_field.span);
                        self.check_doc_attrs(struct_field.node.attrs.as_slice());
                    }
                    ast::Private => {}
                }
            }
            ast::UnnamedField(_) => {}
        }

        // no need to recur; nothing below this level to check.
    }

    /// we're only interested in top-level things, so we can just
    /// ignore these entirely.
    fn visit_local(&mut self, _local: &ast::Local, _env: ()) {}
    fn visit_block(&mut self, _block: &ast::Block, _env: ()) {}
    fn visit_stmt(&mut self, _statement: &ast::Stmt, _env: ()) {}
    fn visit_arm(&mut self, _arm: &ast::Arm, _env: ()) {}
    fn visit_pat(&mut self, _pattern: &ast::Pat, _env: ()) {}
    fn visit_decl(&mut self, _declaration: &ast::Decl, _env: ()) {}
    fn visit_expr(&mut self, _expression: &ast::Expr, _env: ()) {}
    fn visit_expr_post(&mut self, _expression: &ast::Expr, _: ()) {}
    fn visit_ty(&mut self, _typ: &ast::Ty, _env: ()) {}
    fn visit_generics(&mut self, _generics: &ast::Generics, _env: ()) {}
    fn visit_fn(&mut self,
                _function_kind: &visit::FnKind,
                _function_declaration: &ast::FnDecl,
                _block: &ast::Block,
                _span: Span,
                _node_id: ast::NodeId,
                _env: ()) {}
}
