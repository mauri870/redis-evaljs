mod redisjs;
mod vm;

#[macro_use]
extern crate redis_module;

use redis_module::{
    Context, NextArg, RedisError, RedisResult, RedisString, RedisValue, Status, ThreadSafeContext,
};
use rquickjs::{Type, Value as QJSValue};
use crossbeam_channel::Sender;
use std::{cell::RefCell, sync::OnceLock, thread};

static THREAD_POOL: OnceLock<ThreadPool> = OnceLock::new();

// VM is thread-local to avoid contention
thread_local! {
    static VM: RefCell<Option<vm::VM>> = RefCell::new(None);
}

struct ThreadPool {
    sender: Sender<Job>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    fn new(size: usize) -> ThreadPool {
        // Bounded uses crossbeam's array-based flavor (fixed ring buffer), which is
        // significantly cheaper per send/recv than unbounded's segmented-list flavor.
        // The capacity is sized well above realistic in-flight request counts so the
        // queue never actually fills and send() never blocks the Redis main thread.
        let (sender, receiver) = crossbeam_channel::bounded::<Job>(8192);

        for _ in 0..size {
            let receiver = receiver.clone();
            thread::spawn(move || {
                for job in receiver {
                    job();
                }
            });
        }

        ThreadPool { sender }
    }

    fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender.send(job).unwrap();
    }
}

fn init(_ctx: &Context, _args: &[RedisString]) -> Status {
    // Worker threads busy-spin while idle waiting for jobs (crossbeam_channel's
    // backoff strategy). Sizing the pool to num_cpus oversubscribes the machine —
    // idle workers spin-compete with the Redis main thread for cores, which measurably
    // *hurts* throughput. A quarter of the core count keeps enough parallelism for
    // concurrent EVALJS calls without starving everything else.
    THREAD_POOL.get_or_init(|| ThreadPool::new(num_cpus::get().div_ceil(4).max(4)));

    Status::Ok
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

    let pool = THREAD_POOL.get().expect("Thread pool not initialized");
    pool.execute(move || {
        let thread_ctx = ThreadSafeContext::with_blocked_client(blocked_client);

        // Get or create thread-local VM
        let vm_result = VM.with(|vm_cell| {
            let mut vm_opt = vm_cell.borrow_mut();
            if vm_opt.is_none() {
                *vm_opt = Some(vm::VM::new().expect("failed to initialize QJS VM"));
            }
            vm_opt.as_ref().unwrap().with_function(&code, |ctx, func| {
                let globals = ctx.globals();
                globals.set("KEYS", &keys).expect("failed to set KEYS");
                globals.set("ARGV", &argv).expect("failed to set ARGV");

                match func.and_then(|f| f.call(())) {
                    Ok(v) => RedisResult::Ok(Value(v).into()),
                    Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
                }
            })
        });

        thread_ctx.reply(vm_result);
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
            Type::String => RedisValue::BulkString(v.as_string().unwrap().to_string().unwrap()),
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
