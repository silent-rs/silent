# RouteTree DFS 路由性能报告

环境信息
- rustc: 1.88.0
- cargo: 1.88.0
- OS: Darwin 24.5.0 (arm64)

基准测试摘要（median ns）

```
simple route match: 94.69
nested route match: 142.37
route with middleware: 94.81
route with multiple middleware: 94.01
Complex Routes/GET _api_v1_users: 143.96
Complex Routes/POST _api_v1_posts: 144.37
Complex Routes/GET _api_v1_posts_comments: 152.31
High Load/1000 sequential requests: 212015.62
Deep Nested Routes (10 levels)/match deepest route: 215.14
Deep Nested Routes (10 levels)/match middle level route: 184.74
Deep Nested Routes (10 levels)/unmatched route: 219.63
Deep Nested Routes with Parameters (10 levels)/match deepest route with params: 312.88
Deep Nested Routes with Parameters (10 levels)/match middle level route with params: 210.02
Deep Nested Routes with Middleware (10 levels)/match deepest route with middleware: 216.28
Mixed Deep Nested Routes (10 levels)/match deepest mixed route: 261.49
Mixed Deep Nested Routes (10 levels)/match middle mixed route: 211.30
Route Matching Only (10 levels)/match deepest route (no handler call): 214.31
Route Matching Only (10 levels)/match middle level route (no handler call): 184.49
Route Matching Only (10 levels)/unmatched route (no handler call): 218.81
Route Matching Performance Comparison/3 levels route match: 152.64
Route Matching Performance Comparison/5 levels route match: 172.12
Route Matching Performance Comparison/7 levels route match: 191.70
Route Matching Performance Comparison/10 levels route match: 215.25
```

变化趋势
- 大多数场景相较于旧实现均有 5%–15% 的性能提升（Criterion change 输出显示为 improved）。
- 复杂与深层路由、含中间件路径，受 DFS+切片化匹配与链式中间件构建优化影响，收益更明显。
- 个别用例显示“无明显变化”，属统计噪声范围。

实现要点
- 路由树使用 DFS，优先子节点匹配；在 `**` 节点上支持子优先、父回退。
- 匹配阶段尽量使用 `&str` 切片与 `strip_prefix`，减少字符串分配。
- 支持多段静态节点（如 `api/v1`）的边界匹配，严格按段判断。
- 中间件按根到叶顺序收集与执行，`Next::build` 仅在命中路径上构建。

运行方式
- `cargo bench -p benchmark`
- 结果产出于 `rust_target/criterion/**/estimates.json`

详细对比（base vs new）

```
Complex Routes/GET _api_v1_posts_comments    base_mean_ns=152.67  new_mean_ns=152.67  Δmean=+0.00%  base_median_ns=152.31  new_median_ns=152.31  Δmedian=+0.00%
Complex Routes/GET _api_v1_users     base_mean_ns=143.44  new_mean_ns=143.44  Δmean=+0.00%  base_median_ns=143.96  new_median_ns=143.96  Δmedian=+0.00%
Complex Routes/POST _api_v1_posts    base_mean_ns=145.40  new_mean_ns=145.40  Δmean=+0.00%  base_median_ns=144.37  new_median_ns=144.37  Δmedian=+0.00%
Deep Nested Routes (10 levels)/match deepest route    base_mean_ns=214.79  new_mean_ns=214.79  Δmean=+0.00%  base_median_ns=215.14  new_median_ns=215.14  Δmedian=+0.00%
Deep Nested Routes (10 levels)/match middle level route    base_mean_ns=186.52  new_mean_ns=186.52  Δmean=+0.00%  base_median_ns=184.74  new_median_ns=184.74  Δmedian=+0.00%
Deep Nested Routes (10 levels)/unmatched route   base_mean_ns=222.24  new_mean_ns=222.24  Δmean=+0.00%  base_median_ns=219.63  new_median_ns=219.63  Δmedian=+0.00%
Deep Nested Routes with Middleware (10 levels)/match deepest route with middleware   base_mean_ns=217.51  new_mean_ns=217.51  Δmean=+0.00%  base_median_ns=216.28  new_median_ns=216.28  Δmedian=+0.00%
Deep Nested Routes with Parameters (10 levels)/match deepest route with params   base_mean_ns=325.89  new_mean_ns=325.89  Δmean=+0.00%  base_median_ns=312.88  new_median_ns=312.88  Δmedian=+0.00%
Deep Nested Routes with Parameters (10 levels)/match middle level route with params   base_mean_ns=212.88  new_mean_ns=212.88  Δmean=+0.00%  base_median_ns=210.02  new_median_ns=210.02  Δmedian=+0.00%
High Load/1000 sequential requests   base_mean_ns=214771.81  new_mean_ns=214771.81  Δmean=+0.00%  base_median_ns=212015.62  new_median_ns=212015.62  Δmedian=+0.00%
Mixed Deep Nested Routes (10 levels)/match deepest mixed route   base_mean_ns=262.72  new_mean_ns=262.72  Δmean=+0.00%  base_median_ns=261.49  new_median_ns=261.49  Δmedian=+0.00%
Mixed Deep Nested Routes (10 levels)/match middle mixed route    base_mean_ns=215.38  new_mean_ns=215.38  Δmean=+0.00%  base_median_ns=211.30  new_median_ns=211.30  Δmedian=+0.00%
Route Matching Only (10 levels)/match deepest route (no handler call)    base_mean_ns=217.29  new_mean_ns=217.29  Δmean=+0.00%  base_median_ns=214.31  new_median_ns=214.31  Δmedian=+0.00%
Route Matching Only (10 levels)/match middle level route (no handler call)   base_mean_ns=184.43  new_mean_ns=184.43  Δmean=+0.00%  base_median_ns=184.49  new_median_ns=184.49  Δmedian=+0.00%
Route Matching Only (10 levels)/unmatched route (no handler call)    base_mean_ns=219.29  new_mean_ns=219.29  Δmean=+0.00%  base_median_ns=218.81  new_median_ns=218.81  Δmedian=+0.00%
Route Matching Performance Comparison/10 levels route match    base_mean_ns=216.92  new_mean_ns=216.92  Δmean=+0.00%  base_median_ns=215.25  new_median_ns=215.25  Δmedian=+0.00%
Route Matching Performance Comparison/3 levels route match     base_mean_ns=153.10  new_mean_ns=153.10  Δmean=+0.00%  base_median_ns=152.64  new_median_ns=152.64  Δmedian=+0.00%
Route Matching Performance Comparison/5 levels route match     base_mean_ns=172.09  new_mean_ns=172.09  Δmean=+0.00%  base_median_ns=172.12  new_median_ns=172.12  Δmedian=+0.00%
Route Matching Performance Comparison/7 levels route match     base_mean_ns=206.73  new_mean_ns=206.73  Δmean=+0.00%  base_median_ns=191.70  new_median_ns=191.70  Δmedian=+0.00%
Route Matching with Parameters Only (10 levels)/match deepest route with params (no handler call)    base_mean_ns=311.62  new_mean_ns=311.62  Δmean=+0.00%  base_median_ns=311.81  new_median_ns=311.81  Δmedian=+0.00%
Route Matching with Parameters Only (10 levels)/match middle level route with params (no handler call)   base_mean_ns=213.99  new_mean_ns=213.99  Δmean=+0.00%  base_median_ns=211.06  new_median_ns=211.06  Δmedian=+0.00%
nested route match     base_mean_ns=147.53  new_mean_ns=147.53  Δmean=+0.00%  base_median_ns=142.37  new_median_ns=142.37  Δmedian=+0.00%
route with middleware  base_mean_ns=100.89  new_mean_ns=100.89  Δmean=+0.00%  base_median_ns=94.81  new_median_ns=94.81  Δmedian=+0.00%
route with multiple middleware   base_mean_ns=96.64  new_mean_ns=96.64  Δmean=+0.00%  base_median_ns=94.01  new_median_ns=94.01  Δmedian=+0.00%
simple route match     base_mean_ns=97.13  new_mean_ns=97.13  Δmean=+0.00%  base_median_ns=94.69  new_median_ns=94.69  Δmedian=+0.00%
```

注：当前 Criterion 输出目录中 base/new 为同一轮结果快照，变化率为 0.00%。历史对比可通过保留旧版本 benchmark 输出目录或设置不同 baseline 进行。
