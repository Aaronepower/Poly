use std::collections::HashMap;

use serde_json::Value;

use args::Args;

pub type PolyFn = Fn(Vec<Args>);


pub struct Template<'a> {
    variables: Value,
    functions: HashMap<&'a str, Box<PolyFn>>,
}


impl<'a> Template<'a> {
    pub fn load(file: &str) -> Self {}
}
