
use proc_macro2::{Span, TokenStream};
use syn::{*, parse::{ParseStream, Parse}, token::{Brace, Paren}, punctuated::Punctuated, spanned::Spanned};
use quote::{quote, ToTokens, quote_spanned, format_ident};
use convert_case::{Case, Casing};

use crate::EmptyToken;

use super::utils::{attribute_to_string_lit};

pub(crate) trait ToLuaMethod {
    fn to_lua_method(self) -> LuaMethod;
}




#[derive(Debug)]
pub(crate) struct UserdataBlock {
    pub impl_token: Token![impl],
    pub impl_braces: Brace,
    pub functions: Punctuated<LuaMethod,Token![;]>
}

impl Parse for UserdataBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        let f;

        Ok(Self{
            impl_token: input.parse()?,
            impl_braces: braced!(f in input),
            functions: f.parse_terminated(LuaMethod::parse)?,
        })
    }
}

#[derive(Clone,PartialEq,Eq,Hash,Debug)]
pub(crate) struct MethodMacroArg {
    pub ident: Ident,
    pub equals: Token![=],
    pub replacement: TypePath,
}


impl Parse for MethodMacroArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self{
            ident: input.parse()?,
            equals: input.parse()?,
            replacement: input.parse()?
        })
    }
}

impl ToTokens for MethodMacroArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let id = &self.ident;
        let rep = &self.replacement;
        tokens.extend(quote!{
            #id = #rep
        })
    }
}

#[derive(PartialEq,Eq,Clone,Hash,Debug)]
pub(crate) struct LuaMethodType {
    /// does it take &mut  self ?
    pub is_mut : bool,
    /// should it be inlined into the global API ?
    pub is_static: bool,
    /// is it part of the metatable?
    pub is_meta: bool,
    /// does it take self as first parameter?
    pub is_function: bool,

    /// if is_meta this will be Some
    meta_method: Option<TypePath>,
    /// if !is_meta this will be Some
    method_name: Option<LitStr>,
}

impl LuaMethodType {
    pub fn get_inner_tokens(&self) -> TokenStream {
        if self.is_meta {
            return self.meta_method.as_ref().unwrap().into_token_stream()
        } else {
            return self.method_name.as_ref().unwrap().into_token_stream()
        }
    }
}

impl Parse for LuaMethodType {
    fn parse(input: ParseStream) -> Result<Self> {

        let is_static = input.peek(Token![static]).then(|| input.parse::<Token![static]>().unwrap()).is_some();
        let is_mut = input.peek(Token![mut]).then(|| input.parse::<Token![mut]>().unwrap()).is_some();
        let is_function = input.peek(Token![fn]).then(|| input.parse::<Token![fn]>().unwrap()).is_some();

        let mut method_name = None;
        let mut meta_method = None;
        let mut is_meta = false;

        if input.peek(Paren){
            // meta method
            let f;
            parenthesized!(f in input);
            is_meta = true;
            meta_method = Some(f.parse()?);

        } else {
            method_name = Some(input.parse()?);
        }

        Ok(Self{
            is_mut,
            is_static,
            is_meta,
            is_function,
            meta_method,
            method_name,
        })
    
    }
}

impl ToTokens for LuaMethodType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let is_static = self.is_static.then(|| Token![static](tokens.span()));
        let is_mut = self.is_mut.then(|| Token![mut](tokens.span()));
        let is_function = self.is_function.then(|| Token![fn](tokens.span()));
        let mut inner = self.get_inner_tokens();
        if self.is_meta {
            inner = quote!{
                (#inner)
            }
        };
        tokens.extend(quote!{
           #is_static #is_mut #is_function #inner
        })
    }
}

#[derive(Clone,Debug)]
pub(crate) enum LuaClosure {
    MetaClosure{
        paren: Paren,
        args: Punctuated<MethodMacroArg,Token![,]>,
        arrow: Token![=>],
        brace: Brace,
        expr: TokenStream,
    },
    PureClosure{
        arrow: Token![=>],
        expr: ExprClosure
    },
}

impl Parse for LuaClosure {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(token::Paren){
            let f;
            let g;
            Ok(Self::MetaClosure{
                paren: parenthesized!(f in input),
                args: f.parse_terminated(MethodMacroArg::parse)?,
                arrow: input.parse()?,
                brace: braced!(g in input),
                expr: g.parse()?,
            })
        } else {
            
            Ok(Self::PureClosure{
                arrow: input.parse()?,
                expr: input.parse()?
            })
        }
    }
} 

impl LuaClosure {
    pub fn to_applied_closure(&self) -> TokenStream{
        match self {
            LuaClosure::MetaClosure { 
                paren, 
                args, 
                expr,
                .. } => {
                    
                quote!{
                    replace!{#args : #expr}
                }
            },
            LuaClosure::PureClosure { expr, .. } => {
                quote!{
                    #expr
                }
            },
        }
    }
}

impl ToTokens for LuaClosure {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            LuaClosure::MetaClosure { 
                paren, 
                args, 
                expr,
                .. } => {
                    
                tokens.extend(quote!{
                    (#args) => {#expr} 
                })
            },
            LuaClosure::PureClosure { expr, .. } => {
                tokens.extend(quote!{
                    => #expr
                })
            },
        }
    }
}

#[derive(Clone,Debug)]
pub(crate) struct Test {
    pub brace: Brace,
    pub ts: TokenStream
}

impl Parse for Test {
    fn parse(input: ParseStream) -> Result<Self> {
        let f;
        Ok(Self{
            brace: braced!(f in input),
            ts: f.parse()?,
        })
    }
}

impl ToTokens for Test {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ts = &self.ts;
        tokens.extend(quote!{
            {#ts}
        })
    }
}

#[derive(Clone,Debug)]
pub(crate) struct LuaMethod {
    pub docstring: Vec<Attribute>,
    pub method_type: LuaMethodType,
    pub closure: LuaClosure,
    pub test: Option<Test>
}


impl Parse for LuaMethod {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {  
            docstring: Attribute::parse_outer(input)?,
            method_type: input.parse()?,
            closure: input.parse()?,
            test: if input.peek(Token![=>]) {
                input.parse::<Token![=>]>()?;
                Some(input.parse()?)
            } else {None}
        })
    }
}

impl ToTokens for LuaMethod {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ds : Punctuated<Attribute,EmptyToken> = self.docstring.iter().cloned().collect();

        let mt = &self.method_type;
        let closure = &self.closure;
        let test = self.test.as_ref().map(|t| quote!{
            => #t
        });
        tokens.extend(quote!{
            #ds #mt #closure #test
        })
    }
}

impl LuaMethod {


    pub fn gen_tests(&self, newtype_name : &str) -> Option<TokenStream>{
        self.test.as_ref().map(|v| {

            let fun = v.ts.clone();
            let test_ident = Ident::new(&format!{"{}",
                newtype_name.to_case(Case::Snake)
            },Span::call_site()); 

            match &self.closure{
                LuaClosure::MetaClosure { args, .. } => {
                    quote!{
                        replace!{#args : #fun}
                    }},
                LuaClosure::PureClosure {..} => {
                    fun.clone().into_token_stream()
                },
            }
        })
    }

    pub fn to_call_expr(&self,receiver : &'static str) -> TokenStream{


        let closure = &self.closure.to_applied_closure(); 
        let receiver = Ident::new(receiver,Span::call_site());

        let ds : TokenStream = self.docstring.iter().map(|v| {
                let ts : TokenStream = attribute_to_string_lit(v);
                quote!{
                    #receiver.document(#ts);
                }
            }).collect();
        
        let call_ident = format_ident!("add{}{}{}",
            self.method_type.is_meta.then(|| "_meta").unwrap_or(""),
            self.method_type.is_function.then(|| "_function").unwrap_or("_method"),
            self.method_type.is_mut.then(|| "_mut").unwrap_or(""),
        );

        let inner_tokens = self.method_type.get_inner_tokens();


        quote_spanned!{closure.span()=>
            #ds
            #receiver.#call_ident(#inner_tokens,#closure);
        }
    }

    pub fn rebind_macro_args<'a,I: Iterator<Item = &'a MethodMacroArg> + Clone>(&mut self, o : I) -> Result<()>{
        if let LuaClosure::MetaClosure { ref mut args, ..}  = self.closure {
            for other_arg in o{
                // validate the argument
                let corresponding = args.iter_mut()
                    .find(|oa| oa.ident.to_string() == other_arg.ident.to_string())
                    .ok_or(Error::new(Span::call_site(),format!("Invalid argument in macro invocation `{}`. No corresponding variable.",other_arg.ident)))?;

                *corresponding = other_arg.clone()
            }
            Ok(())
        } else {
            Err(Error::new(Span::call_site(),"Attempted to invoke macro args on non-meta closure"))
        }
    }

}



