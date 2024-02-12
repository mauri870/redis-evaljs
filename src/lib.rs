#![feature(once_cell_try)]

#[macro_use]
extern crate redis_module;

use redis_module::{
    logging::RedisLogLevel, CallOptionsBuilder, Context, NextArg, PromiseCallReply, RedisError,
    RedisResult, RedisString, RedisValue, Status, ThreadSafeContext,
};
use rquickjs::{
    prelude::{Func, Rest},
    Context as QJSContext, Ctx, Error as QJSError, FromJs, IntoJs, Object, Result as QJSResult,
    Runtime, Type, Value as QJSValue,
};
use std::{sync::OnceLock, thread};

static QJSCONTEXT: OnceLock<QJSContext> = OnceLock::new();

fn init(redisctx: &Context, _args: &[RedisString]) -> Status {
    let result = QJSCONTEXT.get_or_try_init(|| {
        let rt = Runtime::new()?;
        rt.set_max_stack_size(256 * 1024);
        let ctx = QJSContext::full(&rt)?;
        ctx.with(|ctx| {
            js_module_redis(&ctx)?;
            Ok::<_, QJSError>(())
        })?;

        QJSResult::Ok(ctx)
    });

    match result {
        Ok(_) => Status::Ok,
        Err(e) => {
            redisctx.log_warning(&format!("QJS context init failed: {}", e));
            return Status::Err;
        }
    }
}

pub fn js_module_redis<'js>(ctx: &Ctx<'js>) -> QJSResult<()> {
    let globals = ctx.globals();
    let redis = Object::new(ctx.clone())?;

    // redis.set("call", Func::from(call))?;
    redis.set(
        "call",
        Func::from(
            move |ctx: Ctx<'js>, args: Rest<QJSValue<'js>>| -> QJSResult<String> {
                // TODO: handle variables
                let a: Vec<String> = args.into_iter().map(|v| stringify(&ctx, v)).collect();
                let aa: String = a.join("");
                let rctx = redis_module::MODULE_CONTEXT.lock();
                rctx.log(RedisLogLevel::Warning, aa.as_str());
                // TODO: not sure why this does not work. It might accept only static strings.
                rctx.call("SET", aa.as_str())
                    .expect("failed to run redis call");
                Ok(String::from("call"))
            },
        ),
    )?;
    globals.set("redis", redis)?;

    Ok(())
}

fn evaljs_cmd(redisctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    let mut args = args.into_iter().skip(1);

    let code = match args.next() {
        Some(v) => v.to_string(),
        None => return Err(RedisError::WrongArity),
    };

    let numkeys = args
        .next_u64()
        .map_err(|_| RedisError::Str("ERR invalid number of keys"))? as usize;

    let keys: Vec<String> = args.by_ref().take(numkeys).map(Into::into).collect();
    let argv: Vec<String> = args.map(Into::into).collect();

    let blocked_client = redisctx.block_client();
    thread::spawn(move || {
        let thread_ctx = ThreadSafeContext::with_blocked_client(blocked_client);
        let ctx = match QJSCONTEXT
            .get()
            .ok_or(RedisError::Str("ERR QJS context not initialized"))
        {
            Ok(ctx) => ctx,
            Err(e) => {
                thread_ctx.reply(RedisResult::Err(e));
                return;
            }
        };

        ctx.with(|ctx| {
            let globals = ctx.globals();
            globals
                .set("KEYS", keys.clone())
                .expect("failed to set KEYS");
            globals
                .set("ARGV", argv.clone())
                .expect("failed to set ARGV");
            let envelope = format!(
                r#"
                (function() {{
                    {}
                }})();
            "#,
                code
            );

            let result = match ctx.eval(envelope) {
                Ok(v) => RedisResult::Ok(Value(v).into()),
                Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
            };
            thread_ctx.reply(result)
        });
    });
    Ok(RedisValue::NoReply)

    // let mut result: RedisResult = RedisResult::Ok(RedisValue::Null);

    // let ctx = QJSCONTEXT
    //     .get()
    //     .ok_or(RedisError::Str("ERR QJS context not initialized"))?;
    // ctx.with(|ctx| {
    //     let envelope = format!(
    //         r#"
    //         (function() {{
    //             const KEYS = {};
    //             const ARGV = {};
    //             {}
    //         }})();
    //     "#,
    //         stringify(&ctx, keys),
    //         stringify(&ctx, argv),
    //         code
    //     );

    //     result = match ctx.eval(envelope) {
    //         Ok(v) => RedisResult::Ok(Value(v).into()),
    //         Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
    //     };
    // });

    // result
}

fn stringify<'js>(ctx: &Ctx<'js>, value: impl IntoJs<'js>) -> String {
    ctx.json_stringify(value)
        .expect("failed to stringify")
        .unwrap()
        .to_string()
        .expect("failed to convert to string")
}

struct Value<'a>(QJSValue<'a>);

impl From<Value<'_>> for RedisValue {
    fn from(val: Value) -> Self {
        let v = val.0;
        match v.type_of() {
            Type::Bool => RedisValue::Bool(v.as_bool().unwrap()),
            Type::Int => RedisValue::Integer(v.as_int().unwrap() as i64),
            Type::Float => RedisValue::Float(v.as_float().unwrap()),
            Type::String => RedisValue::BulkString(unsafe { v.ref_string() }.to_string().unwrap()),
            Type::Null | Type::Uninitialized | Type::Undefined | Type::Unknown => RedisValue::Null,
            Type::Array => {
                let arr = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| Value(v.unwrap()).into())
                    .collect();
                RedisValue::Array(arr)
            }
            _ => RedisValue::StaticError("unsupported type"),
        }
    }
}

//////////////////////////////////////////////////////

redis_module! {
    name: "evaljs",
    version: 1,
    allocator: (redis_module::alloc::RedisAlloc, redis_module::alloc::RedisAlloc),
    data_types: [],
    init: init,
    commands: [
        ["EVALJS", evaljs_cmd, "", 0, 0, 0],
    ],
}
