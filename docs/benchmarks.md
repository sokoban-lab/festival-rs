# festival-rs 基准测试

按发布版本记录同机对比结果。表中数值为 Rust / C++，越低越好。逐关数据可用“复现”
中的命令重新生成，历史 Gate 记录见 [`reference/BASELINE.md`](../reference/BASELINE.md)。

## 版本汇总

| 版本  |       日期 |      提交 | Microban 单核 | Microban 8 核 | XSokoban 单核 |   峰值 RSS |
| ----- | ---------: | --------: | ------------: | ------------: | ------------: | ---------: |
| 0.1.0 | 2026-10-29 | `e530e92` |         0.532 |         0.143 |         0.909 | 0.32–0.39× |

单核为 600 秒/关批量结果；8 核为 60 秒/关三次最好值；峰值 RSS 为 Microban 1/8 核
（0.36× / 0.32×）与 XSokoban 批量（0.39×）的比值。

## 0.1.0

### 环境

| 项目           | 值                                                                       |
| -------------- | ------------------------------------------------------------------------ |
| 日期           | 2026-10-29                                                               |
| 提交           | XSokoban `e530e92`；Microban `b567e48`（仅 CI 变更）                     |
| 系统           | Windows 11 Pro 10.0.26300                                                |
| CPU / 内存     | Intel Core i9-14900HX（8 P-core + 16 E-core，32 逻辑核）/ 32 GB          |
| Rust           | rustc 1.99.1；thin LTO、`codegen-units = 1`、`panic = "abort"`、mimalloc |
| C++            | clang++ 23.1.4，`-O3 -DTHREADS`，Festival 3.1 `f7c0231`                  |
| 二进制 SHA-256 | Rust `b3444055…`；C++ `16f46d56…`                                        |
| 关卡集         | Microban I 155 关 `7077dba0…`；XSokoban 3.3c 90 关 `0c6fe8d1…`           |

### 测量方法

- 单核：600 秒/关、默认八策略，固定 CPU 0；整批运行两轮并交换求解器先后。
- 8 核：仅 Microban，60 秒/关，三次；比值受取消时序影响，只作吞吐参考。
- 内存：整批进程峰值工作集，50 ms 采样加退出时的 `PeakWorkingSet64`。
- 质量：比较总步数 / 推动数；所有解用 `festival-bench verify-solutions` 回放。

### Microban I（155 关）

| 指标               |    festival-rs |             C++ |        Rust / C++ |
| ------------------ | -------------: | --------------: | ----------------: |
| 解出               |        155/155 |         155/155 |                 — |
| 单核总耗时（两轮） |  8.936–9.093 s | 16.803–16.925 s |       0.532–0.537 |
| 单核总耗时（最好） |        8.936 s |        16.803 s |         **0.532** |
| 8 核总耗时（三次） | 9.119–10.349 s | 63.839–64.478 s |       0.142–0.161 |
| 8 核总耗时（最好） |        9.119 s |        63.839 s |         **0.143** |
| 步数 / 推动        | 22,486 / 5,920 |  22,460 / 5,912 | 100.12% / 100.14% |
| 峰值工作集（单核） |       264.4 MB |        736.1 MB |             0.359 |
| 峰值工作集（8 核） |     1,843.9 MB |      5,686.5 MB |             0.324 |
| 解回放             |        155/155 |         155/155 |                 — |

### XSokoban（90 关）

| 指标                             |       festival-rs |               C++ |    Rust / C++ |
| -------------------------------- | ----------------: | ----------------: | ------------: |
| 解出                             |             90/90 |             90/90 |             — |
| 单核总耗时（两轮）               | 176.736–185.699 s | 194.512–197.858 s |   0.893–0.955 |
| 单核总耗时（最好）               |         176.736 s |         194.512 s |     **0.909** |
| 步数 / 推动                      |   94,231 / 28,709 |   94,245 / 28,709 | 99.99% / 100% |
| 峰值工作集（批量）               |          339.6 MB |          870.8 MB |         0.390 |
| 峰值工作集（单关最大，第 29 关） |          290.4 MB |          843.8 MB |         0.344 |
| 解回放                           |             90/90 |             90/90 |             — |

### 复现

```sh
cargo build --release -p festival-cli -p festival-bench
scripts/build_cpp_ref.sh
sha256sum -c levels/corpus/microban.xsb.sha256
sha256sum -c levels/corpus/xsokoban.sok.sha256

# 单核批量（固定 CPU 0）
mkdir -p out/mb-rust out/mb-cpp out/xs-rust out/xs-cpp
target/release/festival-rs.exe levels/corpus/microban.xsb --level 1..155 --time 600 --cores 1 --out-dir out/mb-rust
target/festival-cpp.exe levels/corpus/microban.xsb -from 1 -to 155 -time 600 -cores 1 -out_dir out/mb-cpp
target/release/festival-rs.exe levels/corpus/xsokoban.sok --level 1..90 --time 600 --cores 1 --out-dir out/xs-rust
target/festival-cpp.exe levels/corpus/xsokoban.sok -from 1 -to 90 -time 600 -cores 1 -out_dir out/xs-cpp

# 8 核 Microban（重复三次）
mkdir -p out/mb-rust8 out/mb-cpp8
target/release/festival-rs.exe levels/corpus/microban.xsb --level 1..155 --time 60 --cores 8 --out-dir out/mb-rust8
target/festival-cpp.exe levels/corpus/microban.xsb -from 1 -to 155 -time 60 -cores 8 -out_dir out/mb-cpp8

# 峰值内存
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/process_rss.ps1 -Out out/rss.log -Exe target/release/festival-rs.exe -ArgList "levels/corpus/microban.xsb|--level|1..155|--time|600|--cores|1|--out-dir|out/mb-rss"

# 回放
target/release/festival-bench.exe verify-solutions levels/corpus/microban.xsb out/mb-rust/solutions.sok
target/release/festival-bench.exe verify-solutions levels/corpus/xsokoban.sok out/xs-rust/solutions.sok
```
