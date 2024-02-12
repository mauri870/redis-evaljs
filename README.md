# EVALJS Redis module

This module provides a way to evaluate JavaScript code inside of Redis. It uses the awesome QuickJS engine by Fabrice Bellard.

It is similar to [EVAL](https://redis.io/commands/eval), but EVALJS is very basic and slow in its current state. There is no support for executing Redis commands nor handling KEYS and ARGV. Think about it as a JS interpreter inside Redis.

```bash
$ redis-cli EVALJS "return 1 + 2" 0
(integer) 3

$ redis-cli EVALJS "return 'Hello JS!'" 0
"Hello JS!"

$ redis-cli EVALJS "const fib = n => n <= 1 ? n : fib(n - 1) + fib(n - 2); return fib(10)" 0
(integer) 55

$ redis-cli EVALJS "return ['a', 'c', 'b'].sort((a, b) => a - b)" 0
1) "a"
2) "c"
3) "b"
```

## Installation

You can build the module using cargo:

```sh
cargo build --release
```

Then you can load the module into a Redis server with:

```sh
redis-server --loadmodule ./target/release/libredisjs.so

# or
redis-cli MODULE LOAD ./target/release/libredisjs.so
```