#[macro_use]
extern crate redis_module;

use redis_module::{Context, NextArg, RedisError, RedisResult, RedisString, RedisValue, Status};
use rquickjs::{Context as QJSContext, Ctx, IntoJs, Runtime, Type, Value as QJSValue};
use std::sync::OnceLock;

static QJSCONTEXT: OnceLock<QJSContext> = OnceLock::new();

fn init(_ctx: &Context, _args: &[RedisString]) -> Status {
    QJSCONTEXT.get_or_init(|| {
        let rt = Runtime::new().unwrap();
        rt.set_max_stack_size(256 * 1024);
        let ctx = QJSContext::full(&rt).unwrap();
        ctx
    });

    Status::Ok
}
fn evaljs_cmd(_ctx: &Context, args: Vec<RedisString>) -> RedisResult {
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

    let mut result: RedisResult = RedisResult::Ok(RedisValue::Null);

    let ctx = QJSCONTEXT
        .get()
        .ok_or(RedisError::Str("ERR QJS context not initialized"))?;
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

        result = match ctx.eval(envelope) {
            Ok(v) => RedisResult::Ok(Value(v).into()),
            Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
        };
    });

    result
}

fn stringify<'js>(ctx: &Ctx<'js>, value: impl IntoJs<'js>) -> String {
    ctx.json_stringify(value)
        .unwrap()
        .unwrap()
        .to_string()
        .unwrap()
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
