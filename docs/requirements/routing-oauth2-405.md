# 路由需求-修复 /oauth2/applications 返回405

- 背景：在如下路由结构下，请求 GET /oauth2/applications 出现 405 Method Not Allowed。

```
Route::new("oauth2")
    .append(
        Route::new("applications")
            .get(get_applications)
            .post(create_application)
            .append(
                Route::new("<id:str>")
                    .get(get_application)
                    .put(update_application)
                    .delete(delete_application)
                    .append(Route::new("status").patch(update_status))
                    .append(Route::new("regenerate-secret").post(regenerate_secret))
                    .append(Route::new("access-config").put(update_access_config)),
            ),
    )
```

- 期望：GET /oauth2/applications 命中 applications 节点的 GET 处理器，返回 200。
- 现象：匹配误入子节点 <id:str>，导致处理器选择异常并最终返回 405。

## 根因

- 路由特殊段 <id:str> 在匹配实现中将空路径段视为可匹配（将空字符串注入路径参数），
  当请求正好落在父节点（applications）时，DFS 会继续尝试子节点 <id:str> 并误判匹配成功，
  从而覆盖了父节点的处理器选择。

## 需求与修复要点

- <key:str> 类型的特殊段匹配必须要求当前路径段非空；空段不应匹配。
- 其他数值类型（<id:int>、<id:i64> 等）因解析失败已自然不匹配，此变更与其语义一致。
- 修复后应增加单元测试，覆盖 GET /oauth2/applications 场景，确保不再返回 405。

## 验收

- 运行 cargo test -p silent --tests 全部通过。
- GET /oauth2/applications 命中父节点 applications 的 GET 处理器。
