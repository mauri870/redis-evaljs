use rquickjs::{Context, Ctx, Function, Persistent, Runtime};

use std::{cell::RefCell, collections::HashMap, result::Result as StdResult};

pub struct VM {
    _runtime: Runtime,
    ctx: Context,
    // Compiled scripts, keyed by their exact source text
    functions: RefCell<HashMap<String, Persistent<Function<'static>>>>,
}

impl VM {
    pub fn new() -> StdResult<Self, Box<dyn std::error::Error + Send + Sync>> {
        let runtime = Runtime::new().unwrap();
        runtime.set_max_stack_size(256 * 1024);
        let ctx = Context::full(&runtime)?;

        ctx.with(|ctx| {
            crate::redisjs::init(&ctx)?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })?;

        Ok(Self {
            _runtime: runtime,
            ctx,
            functions: RefCell::new(HashMap::new()),
        })
    }

    /// Runs `f` with the compiled function for `code`, compiling and caching
    /// it on first use. `f` is responsible for setting up any globals the
    /// script expects (KEYS/ARGV) before calling the function.
    pub fn with_function<F, R>(&self, code: &str, f: F) -> R
    where
        F: FnOnce(Ctx, rquickjs::Result<Function>) -> R,
    {
        self.ctx.with(|ctx| {
            let func = self.get_or_compile(&ctx, code);
            f(ctx, func)
        })
    }

    fn get_or_compile<'js>(&self, ctx: &Ctx<'js>, code: &str) -> rquickjs::Result<Function<'js>> {
        if let Some(cached) = self.functions.borrow().get(code) {
            return cached.clone().restore(ctx);
        }

        let mut wrapper = String::with_capacity(code.len() + 14);
        wrapper.push_str("(function(){");
        wrapper.push_str(code);
        wrapper.push_str("})");

        let func: Function = ctx.eval(wrapper)?;
        let persistent = Persistent::save(ctx, func.clone());
        self.functions.borrow_mut().insert(code.to_string(), persistent);
        Ok(func)
    }
}
