#![feature(once_cell_try)]

mod redisjs;
mod vm;

#[macro_use]
extern crate redis_module;

use redis_module::{
    Context, NextArg, RedisError, RedisResult, RedisString, RedisValue, Status, ThreadSafeContext,
};
use rquickjs::{Type, Value as QJSValue};
use std::{sync::OnceLock, thread};

static VM: OnceLock<vm::VM> = OnceLock::new();

fn init(ctx: &Context, _args: &[RedisString]) -> Status {
    let result = VM.get_or_try_init(|| vm::VM::new());

    match result {
        Ok(_) => Status::Ok,
        Err(e) => {
            ctx.log_warning(&format!("VM initialization failed: {}", e));
            Status::Err
        }
    }
}

fn evaljs_cmd(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
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

    let blocked_client = ctx.block_client();
    thread::spawn(move || {
        let thread_ctx = ThreadSafeContext::with_blocked_client(blocked_client);
        let vm = match VM
            .get()
            .ok_or(RedisError::Str("ERR QJS context not initialized"))
        {
            Ok(vm) => vm,
            Err(e) => {
                thread_ctx.reply(RedisResult::Err(e));
                return;
            }
        };

        vm.eval(|ctx| {
            let globals = ctx.globals();
            globals
                .set("KEYS", keys.clone())
                .expect("failed to set KEYS");
            globals
                .set("ARGV", argv.clone())
                .expect("failed to set ARGV");
            let wrapper = format!(
                r#"
                (function() {{
                    {}
                }})();
            "#,
                code
            );

            let result = match ctx.eval(wrapper) {
                Ok(v) => RedisResult::Ok(Value(v).into()),
                Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
            };
            thread_ctx.reply(result)
        });
    });
    Ok(RedisValue::NoReply)
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
