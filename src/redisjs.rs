use rquickjs::{
    prelude::{Func, Rest},
    Ctx, IntoJs, Object, Result, Value,
};

pub fn init(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();

    let redis = Object::new(ctx.clone())?;

    redis.set("call", Func::from(call))?;

    globals.set("redis", redis)?;

    Ok(())
}

fn call<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> Result<Value<'js>> {
    let strargs = args
        .iter()
        .map(|v| unsafe { v.ref_string() }.to_string().unwrap())
        .collect::<Vec<_>>();
    let cmdargs: Vec<&String> = strargs.iter().collect();

    let rctx = redis_module::MODULE_CONTEXT.lock();
    let res = rctx
        .call(cmdargs[0], &cmdargs[1..])
        .expect("failed to call redis");

    match res {
        redis_module::RedisValue::SimpleString(s) => s.into_js(&ctx),
        _ => Ok(Value::new_null(ctx)),
    }
}
