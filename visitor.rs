use std::hashmap;
use std::ascii::StrAsciiExt;

use syntax::visit;
use syntax::visit::Visitor;
use syntax::parse::token;
use syntax::codemap::span;
use syntax::ast::*;
use syntax::attr::AttrMetaMethods;

use words;

pub struct SpellingVisitor {
    words: hashmap::HashSet<~str>,
    misspellings: hashmap::HashMap<span, hashmap::HashSet<~str>>
}

impl SpellingVisitor {
    pub fn new(words: hashmap::HashSet<~str>) -> SpellingVisitor {
        SpellingVisitor { words: words, misspellings: hashmap::HashMap::new() }
    }

    fn raw_word_is_correct(&mut self, w: &str) -> bool {
        self.words.contains_equiv(&w) ||
            !w.iter().all(|c| c.is_alphabetic()) ||
            self.words.contains_equiv(&w.to_ascii_lower())
    }

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

    fn check_ident(&mut self, id: ident, sp: span) {
        let word = token::ident_to_str(&id);
        if word.starts_with("__") { return }
        if self.raw_word_is_correct(word) {
            return
        }

        self.check_subwords(word, sp);
    }

    fn check_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs.iter() {
            match attr.name_str_pair() {
                Some((doc, doc_str)) if "doc" == doc => {
                    self.check_subwords(doc_str, attr.span);
                }
                _ => {}
            }
        }
    }

    pub fn check_crate(@mut self, crate: &Crate) {
        self.check_attrs(crate.attrs);
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
    fn visit_view_item(@mut self, view_item: &view_item, env: ()) {
        self.check_attrs(view_item.attrs);
        visit::visit_view_item(self as @mut Visitor<()>, view_item, env)
    }
    fn visit_foreign_item(@mut self, foreign_item: @foreign_item, env: ()) {
        self.check_attrs(foreign_item.attrs);
        visit::visit_foreign_item(self as @mut Visitor<()>, foreign_item, env)
    }
    fn visit_item(@mut self, item: @item, env: ()) {
        match item.vis {
            public | inherited => {
                self.check_attrs(item.attrs);
                self.check_ident(item.ident, item.span);
            }
            // no need to check the names/docs of private things
            // (although there may be public things inside them that
            // are re-exported somewhere else, so still recur)
            private => {}
        }
        match item.node {
            // no visitor method for enum variants.
            item_enum(ref ed, _) => {
                for var in ed.variants.iter() {
                    self.check_ident(var.node.name, var.span);
                    self.check_attrs(var.node.attrs);
                }
            }
            _ => {}
        }

        visit::visit_item(self as @mut Visitor<()>, item, env)
    }

    fn visit_ty_method(@mut self, method_type: &TypeMethod, env: ()) {
        self.check_attrs(method_type.attrs);
        self.check_ident(method_type.ident, method_type.span);
        visit::visit_ty_method(self as @mut Visitor<()>, method_type, env)
    }
    fn visit_trait_method(@mut self, trait_method: &trait_method, env: ()) {
        match *trait_method {
            required(_) => {}
            provided(method) => {
                self.check_ident(method.ident, method.span);
                visit::visit_trait_method(self as @mut Visitor<()>, trait_method, env)
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
    fn visit_struct_field(@mut self, struct_field: @struct_field, env: ()) {
        match struct_field.node.kind {
            named_field(id, _) => {
                self.check_ident(id, struct_field.span)
            }
            unnamed_field => {}
        }
        self.check_attrs(struct_field.node.attrs);
        visit::visit_struct_field(self as @mut Visitor<()>, struct_field, env)
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
