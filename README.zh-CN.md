# arboretum

[English](README.md) · **简体中文**

**运行在链上的确定性衍生品定价。Rust 实现，全篇没有浮点数。**

期权价格通常是一个由别人在链下算好、再通过预言机送进你合约的数字。这里把它变成一个任何人都能重算的数字：给定同样的六个输入，每一个验证者节点执行同样的整数运算，得到同样的结果，逐位相同。

本仓库是 **Arbitrum Open House Singapore — Online Buildathon** 的参赛作品。

---

## 完成状态

| | |
|---|---|
| `arbnum` — 定点数学 | **完成**，15 个测试 |
| `arbpricing` — Black-Scholes、希腊字母、CRR 格点、隐含波动率 | **完成**，20 个测试 |
| `arbreport` — 对照 Python 参考实现的精度检验 | **完成**，3336 个用例，0 个超出预算 |
| `arbcontract` — Stylus 合约 | **已部署**，压缩后 14,668 字节 |
| `arbwasm` — 体积探针，用来归因 SDK 自身占了多少 | 完成，压缩后 14.6 KB |
| 测试网部署 | **已在 Arbitrum Sepolia 上线**，`0x374f469725d735115b8b15dee3f8749ff929d94a` |
| `web/` — 演示页面 | **已上线** <https://arboretum-pricing-vkk9ilpssgw.qoder.zone> |
| 演示视频 | **已录制**，随本次提交发布在 HackQuest 参赛页上；录制清单见 [`docs/VIDEO.md`](docs/VIDEO.md)（英文） |
| 蒙特卡洛定价 | **有意不做**。在没有跳跃模型或随机波动率模型的前提下，模拟几何布朗运动只会用更慢的速度复现闭式解。等到出现需要它的收益结构时再做，现在没有。 |

`cargo test` 全绿。`docs/ACCURACY.md` 由 `cargo run -p arbreport` 生成，不是手写的，所以下面的数字不会和代码脱节。

部署需要对官方 CLI 打一个可移植性补丁，因为它发布的源码在 Windows 上根本编译不过。详见
[`docs/DEPLOY.md`](docs/DEPLOY.md) 和 `scripts/patches/cargo-stylus-windows.patch`。

## 为什么要放到链上，以及这个说法在哪里不成立

诚实版本的说法比"更便宜"或"更快"要窄，而且这个窄是刻意的。

现在的链上期权协议，标记价格来自一个链下模型。也就是说，决定你被清算和被结算的那个数字，来自一段你既看不到也无法复现的计算，链只是执行了这个断言。把这段计算搬到链上，价格就变成一个第三方可以用同样的程序、从公开输入重新推导出来的东西。

这个性质的代价是比信任预言机高一到两个数量级的 gas，所以它应该用在下注足够大的地方：**结算价和到期价、备用标记价、可复现的审计、以及结构化产品的构建**。也就是每个市场一次、金额大、有对手方的场景。连续的报价应该留在链下。这个 crate 不打算和做市商竞争。

## 用法

```bash
cargo test                          # 宿主 crate 共 36 个测试
cargo clippy --all-targets          # 无警告
cargo run -p arbreport              # 重新生成 docs/ACCURACY.md
py reference/gen_vectors.py         # 重新生成参考向量（Python 3）

cd crates/arbcontract               # 合约是独立 workspace：只出 wasm
cargo build --release --target wasm32-unknown-unknown --lib
cd ../..

scripts/size.sh                     # 原始体积与 brotli 体积，合约与内核
scripts/verify_no_floats.sh         # 校验 + 反汇编，出现任何 f32/f64 即失败
```

关于目录结构：`arbcontract` 被排除在根 workspace 之外，并且自带 `[profile.release]`。把它链接到宿主平台会失败，这是设计如此；继承宿主的 profile 会多占掉大约三分之一的产物体积。

`--lib` 不是装饰。这个包里还有一个 bin 目标，就是那个构造函数探针，两者写到同一个 `arbcontract.wasm` 路径上，所以不指定目标去构建，留在原地的就是最后编译完的那个，而 bin 只有 368 字节。拿一个空壳去跑体积检查或者无浮点检查，会得到一个什么都没证明的 PASS。现在两个脚本都按名字指定 lib 目标，并且 `verify_no_floats.sh` 会拒绝任何没有导出 `user_entrypoint` 的模块。

```rust
use arbnum::D;
use arbpricing::{european, greeks, binomial, implied_vol, bounds, Kind, Market};

// 现货 250，行权价 240，三个月，波动率 35%，利率 5%，持有成本 1% —— 全部为 1e9 标度。
let m = Market {
    spot: D::from_raw(250_000_000_000),
    strike: D::from_raw(240_000_000_000),
    t: D::from_raw(250_000_000),       // 0.25 年
    sigma: D::from_raw(350_000_000),
    rate: D::from_raw(50_000_000),
    carry: D::from_raw(10_000_000),
};
let call = european(&m, Kind::Call)?;
let american = binomial(&m, Kind::Put, 512, true)?;
let vol = implied_vol(call, m, Kind::Call)?;
```

## 演示页面

**地址 <https://arboretum-pricing-vkk9ilpssgw.qoder.zone>**，不需要钱包。

`web/` 是一个 Next.js 应用，让评审可以在不信任我们技术栈任何一部分的情况下验证整个说法：选一个期权，页面去问已部署的合约，然后把答案和同一个用例由本仓库 Rust 构建算出的结果并排展示。MATCH 标记表示这两个整数相等，没有施加任何容差。

它有意不包含任何定价代码。一份 JavaScript 的重新实现，会变成对同一个问题的第二个浮点答案，而这个问题唯一的可取之处就是它只有一个答案。所以宿主侧以数据的形式发布，由 `scripts/gen_parity.sh` 生成（1680 个欧式用例，加上格点面板那组固定参数）。

页面还会打印它发出的调用的完整 calldata，所以同样的值可以用 `eth_call` 从任意节点取到。

```bash
cd web && npm install && npm run dev     # http://localhost:3000
```

读取请求从浏览器直接发往公开的 Arbitrum Sepolia RPC，并在三个互相独立的端点之间做了回退：官方端点走的是负载均衡，它的后端有时会重复返回 `access-control-allow-origin` 响应头，浏览器会拒绝这种响应。项目构建成静态导出，所以这条链路上没有我们自己的服务器。

## 到底保证了什么

**没有浮点数。** Arbitrum 的 Stylus 运行时把 `f32`/`f64` 列为不支持
（<https://docs.arbitrum.io/stylus/concepts/webassembly>），所以引擎用 1e9 标度的有符号定点数（`i128`）写成，并固定了一条 round half up 规则。浮点只出现在 `#[cfg(test)]` 的代码里和只跑在宿主的 `arbreport` 二进制里，两者都到不了验证者节点。这才让结果真正可复现，而不只是看起来确定。`scripts/verify_no_floats.sh` 检查的是编译产物，不是源码。

**有界且带检查的算术。** 每一个运算都受检。溢出是错误，不会回绕。乘积放不下时会走拆分路径；拆分后仍然放不下，调用就带着原因 revert。

**没有第三方依赖。** 没有从 crates.io 拉任何东西：初等函数全部在这里实现，一方面为了可审计，另一方面 Stylus 的代码体积上限会惩罚引入通用数学 crate 的做法。`exp`、`ln`、`sqrt`、`pow`、`erf`、`norm_cdf`、`norm_pdf` 都是手写的级数或有理式。

**精度是量出来的，不是声明出来的。** 3336 个来自 CPython `math` 模块的参考用例，每个参考值都在量化后的输入上求值，这样量到的是近似误差，而不是输入舍入。预算是**推导**出来的：价格预算是正态 CDF 的公开误差经过公式的两项传播得到的，不是一个凑出来的常数。

| 分组 | 用例数 | 实测 |
|---|---:|---|
| `exp` | 276 | 相对误差 2.2e-9 |
| `ln` | 300 | 相对误差 1.7e-8 |
| `sqrt` | 300 | 相对误差 7.0e-7 |
| `Phi` | 560 | 绝对误差 7.55e-8 |
| Black-Scholes + 希腊字母 | 1480 | 每个用例都在推导带内 |
| CRR 格点（欧式 + 美式） | 10 | 相对误差 1.4e-6 |
| 隐含波动率 | 10 | sigma 上 8.0e-6 |

## 体积

Stylus 限制的是**压缩后**的程序体积。`scripts/size.sh` 量的是这个，官方 CLI 量的是 `wasm-opt` 之后的结果，会更小：

| 产物 | 原始 | brotli -q11 | 官方 CLI |
|---|---:|---:|---:|
| `arbcontract` — 合约，所有定价路径都暴露 | 69.5 KB | 18.4 KB | **14,668 字节** |
| `arbwasm` — 只有内核，用于归因 | 48.2 KB | 14.6 KB | — |

上限：96 KB（ArbOS Elara，2026-08-20），此前是 24 KB。两个产物在 Elara 之前的上限下也放得下，这意味着这套引擎可以部署在尚未升级的链上。

```
$ cargo stylus deploy -e https://sepolia-rollup.arbitrum.io/rpc --no-verify...
contract size: 14.7 KB (14668 bytes)
wasm data fee: 0.000108 ETH (originally 0.000090 ETH with 20% bump)
deployed code at address: 0x374f469725d735115b8b15dee3f8749ff929d94a
successfully activated contract 0x374f469725d735115b8b15dee3f8749ff929d94a
```

**已部署的合约和这份源码在本机构建的产物，逐位一致。**

```
$ scripts/verify_onchain.sh
PASS  priceEuropean_call           23843783735
PASS  priceLattice_amer_put_512    11622510326
PASS  impliedVol_from_23.8         349054514
...
all cases agree: the deployed contract and this host build are bit-identical
```

这就是整个项目的论点，被写成了任何人都能跑的一条命令：不是"相信这个数"，而是"跑一遍程序，得到同一个数"。部署它需要一个有资金的密钥和六处彼此独立的工具链配置，全部记录在
[`docs/DEPLOY.md`](docs/DEPLOY.md) 里，包括官方 CLI 为了能在 Windows 上编译所必需的那个单文件补丁。

`scripts/verify_no_floats.sh` 校验编译出的模块并把它反汇编：WebAssembly 里每一个浮点类型的值在文本形式里都会写成 `f32` 或 `f64`，所以"没有浮点"这个说法是对着产物检查出来的，不是对着源码声明出来的。

## 已知限制

直说，因为一个藏起自己边界的项目，比一个把边界写出来的项目更不可信。

1. **波动率是输入，而代币化股票目前没有可供读取的实时隐含波动率曲面。** 这里的贡献是"从输入到价格"这段确定性变换，不是数据源。公开的波动率曲面应当放在带签名或 Merkle 承诺的输入之后，这个方案设计了，但还没实现。
2. **正态 CDF 用的是 Abramowitz & Stegun 26.2.17**（公开误差 7.5e-8 绝对值，这里实测 7.55e-8）。在 250 的标的上，它精确到大约万分之零点二。它不是正确舍入的，对波动率的单调性也只保证在这个误差范围内，不是严格单调。
3. **深度虚值的权利金低于约 1e-9 时会归零。** 闭式解在那里是两个相互抵消的项之差；引擎能识别这种抵消并归零，而不是返回噪声，而这个宽度的噪声带是从 CDF 误差推导出来的。
4. **没有跳跃扩散、随机波动率或障碍模型。** 这是 Black-Scholes 和 CRR 这一族，做得仔细，不是一整套模型库。
5. **未审计。** 合约部署在 Arbitrum Sepolia 测试网上，没有上主网，也没有经过任何外部审查。

## 现有技术

有三件事会让"首创"这个说法不成立，所以这里不做任何首创声明：

- **Lyra 的第一个版本就在合约内部运行 Black-Scholes。** 链上定价不是新东西。
- **Offchain Labs 在 2024 年 10 月演示过 Stylus 里的 Black-Scholes**（Stylus Pro Series 第 3 天），这件事列在他们自己的 `awesome-stylus` 里。所以"第一个在 Stylus 里写 Black-Scholes"同样不成立。
- **`chrisco512/black_scholes`**（2024 年 10 月）是最接近的公开仓库：只有欧式定价，基于 `rust_decimal` 而不是整数定点，从未部署，没有演示页面。

同一场 buildathon 里还有 **`afterhours`**（2026 年 9 月），一个给看跌期权定价并封装 ERC-4626 金库的 Stylus 合约。

以上这些都没有覆盖、而本仓库做了的是：闭式解、五个希腊字母、带提前行权的 CRR 格点、隐含波动率反演，以及一个推导出来的不确定带，五者同时具备；纯整数运算，并且"没有浮点"是对着编译产物验证的而不是对着源码；一个已部署的合约，逐入口点与本地构建比对；以及一个自己产出自己数字的精度检验。

## 许可

MIT。
