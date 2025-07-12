# EVALJS Redis module

This module provides a way to evaluate JavaScript code inside of Redis. It uses the awesome QuickJS engine by Fabrice Bellard.

It is similar in functionality to [EVAL](https://redis.io/commands/eval). Currently there is no support for executing Redis commands, think of it as a JS interpreter inside Redis.

Unfortunately it is not as fast as EVAL, but it is still quite fast:

```bash
$ redis-benchmark EVALJS "return 1 + 2" 0
Summary:
  throughput summary: 44169.61 requests per second
  latency summary (msec):
          avg       min       p50       p95       p99       max
        0.919     0.240     0.871     1.423     1.671     2.367

$ redis-benchmark EVAL "return 1 + 2" 0
Summary:
  throughput summary: 58105.75 requests per second
  latency summary (msec):
          avg       min       p50       p95       p99       max
        0.654     0.240     0.591     1.127     1.343     2.023
```

Here are some examples:

```bash
$ redis-cli EVALJS "return 1 + 2" 0
(integer) 3

$ redis-cli EVALJS "return 'Hello JS!'" 0
"Hello JS!"

$ redis-cli EVALJS "const fib = n => n <= 1 ? n : fib(n - 1) + fib(n - 2); return fib(10)" 0
(integer) 55

$ redis-cli EVALJS "return [5, 4, 3, 2, 1].sort((a, b) => a - b)" 0
1) (integer) 1
2) (integer) 2
3) (integer) 3
4) (integer) 4
5) (integer) 5

$ redis-cli EVALJS "return [KEYS[0], KEYS[1], ARGV[0], ARGV[1], ARGV[2]]" 2 key1 key2 arg1 arg2 arg3
1) "key1"
2) "key2"
3) "arg1"
4) "arg2"
5) "arg3"

$ redis-cli EVALJS "return redis.call('SET', 'a', 42)" 0
"OK"

$ redis-cli EVALJS "return redis.call('GET', 'a')" 0
"42"
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

## Testing

After starting a Redis server with the module loaded, you can run the tests with [bats](https://github.com/bats-core/bats-core):

```sh
bats tests
```
