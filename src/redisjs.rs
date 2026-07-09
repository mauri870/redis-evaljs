use redis_module::RedisValue;
use rquickjs::{
    prelude::{Func, Rest},
    Ctx, Exception, IntoJs, Object, Result, Type, Value,
};
use smallvec::SmallVec;

pub fn init(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();

    let redis = Object::new(ctx.clone())?;

    redis.set("call", Func::from(call))?;

    globals.set("redis", redis)?;

    Ok(())
}

// Stringify a redis.call argument the way EVAL/Lua does: strings pass through,
// numbers and booleans are coerced to their string form. Anything else (e.g.
// an object) can't be sent to Redis as a command argument.
fn arg_to_string(v: &Value) -> Result<String> {
    match v.type_of() {
        Type::String => Ok(unsafe { v.ref_string() }.to_string()?),
        Type::Int => Ok(v.as_int().unwrap().to_string()),
        Type::Float => Ok(v.as_float().unwrap().to_string()),
        Type::Bool => Ok(if v.as_bool().unwrap() { "1" } else { "0" }.to_string()),
        other => Err(Exception::throw_type(
            v.ctx(),
            &format!("redis.call: unsupported argument type {other:?}"),
        )),
    }
}

fn call<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> Result<Value<'js>> {
    let strargs: SmallVec<[String; 8]> = args
        .iter()
        .map(arg_to_string)
        .collect::<Result<_>>()?;

    if strargs.is_empty() {
        return Err(Exception::throw_type(&ctx, "redis.call: no command given"));
    }

    let cmdargs: SmallVec<[&str; 8]> = strargs.iter().map(String::as_str).collect();

    let res: RedisValue = {
        let rctx = redis_module::MODULE_CONTEXT.lock();
        rctx.call(cmdargs[0], &cmdargs[1..])
            .expect("failed to call redis")
    };

    // Handle more Redis return types
    match res {
        redis_module::RedisValue::SimpleString(s) => s.into_js(&ctx),
        redis_module::RedisValue::BulkString(s) => s.into_js(&ctx),
        redis_module::RedisValue::Integer(i) => i.into_js(&ctx),
        redis_module::RedisValue::Float(f) => f.into_js(&ctx),
        redis_module::RedisValue::Bool(b) => b.into_js(&ctx),
        redis_module::RedisValue::Array(arr) => {
            let js_arr = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in arr.iter().enumerate() {
                match item {
                    redis_module::RedisValue::SimpleString(s) => js_arr.set(i, s)?,
                    redis_module::RedisValue::BulkString(s) => js_arr.set(i, s)?,
                    redis_module::RedisValue::Integer(n) => js_arr.set(i, *n)?,
                    redis_module::RedisValue::Float(f) => js_arr.set(i, *f)?,
                    redis_module::RedisValue::Bool(b) => js_arr.set(i, *b)?,
                    _ => js_arr.set(i, Value::new_null(ctx.clone()))?,
                }
            }
            Ok(js_arr.into_value())
        }
        redis_module::RedisValue::Null => Ok(Value::new_null(ctx)),
        _ => Ok(Value::new_null(ctx)),
    }
}
