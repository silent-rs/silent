use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

type AnyMap = HashMap<TypeId, Arc<dyn Any + Send + Sync>, BuildHasherDefault<IdHasher>>;

// With TypeIds as keys, there's no need to hash them. They are already hashes
// themselves, coming from the compiler. The IdHasher just holds the u64 of
// the TypeId, and then returns it, instead of doing any bit fiddling.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }
}

/// 类型安全的键值存储容器。
///
/// `State` 用于按类型存储和检索值，是 `State` 和 `Configs` 的底层实现。
/// 可被 `Request` 和 `Response` 用来存储从底层协议派生的额外数据。
#[derive(Default, Clone)]
pub struct State {
    // If extensions are never used, no need to carry around an empty HashMap.
    // That's 3 words. Instead, this is only 1 word.
    map: Option<Box<AnyMap>>,
}

/// `Configs` 是 `State` 的类型别名，保持向后兼容。
///
/// **已弃用**：请使用 `State<T>` 提取器代替 `Configs<T>` 提取器。
/// `Configs` 将在 v2.18.0 中移除。
#[deprecated(
    since = "2.16.0",
    note = "请使用 State<T> 提取器代替，Configs 将在 v2.18.0 移除"
)]
pub type Configs = State;

impl State {
    /// Create an empty `State`.
    #[inline]
    pub fn new() -> State {
        State { map: None }
    }

    /// Insert a type into this `State`.
    ///
    /// If an extension of this type already existed, it will
    /// be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// assert!(cfg.insert(5i32).is_none());
    /// assert!(cfg.insert(4u8).is_none());
    /// assert_eq!(cfg.insert(9i32), Some(5i32));
    /// ```
    pub fn insert<T: Send + Sync + Clone + 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .get_or_insert_with(Box::default)
            .insert(TypeId::of::<T>(), Arc::new(val))
            .and_then(|boxed| (boxed as Arc<dyn Any + 'static>).downcast_ref().cloned())
    }

    /// Get a reference to a type previously inserted on this `State`.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// assert!(cfg.get::<i32>().is_none());
    /// cfg.insert(5i32);
    ///
    /// assert_eq!(cfg.get::<i32>(), Some(&5i32));
    /// ```
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(&TypeId::of::<T>()))
            .and_then(|boxed| (&**boxed as &(dyn Any + 'static)).downcast_ref())
    }

    /// Remove a type from this `State`.
    ///
    /// If aa extension of this type existed, it will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// cfg.insert(5i32);
    /// assert_eq!(cfg.remove::<i32>(), Some(5i32));
    /// assert!(cfg.get::<i32>().is_none());
    /// ```
    pub fn remove<T: Send + Sync + Clone + 'static>(&mut self) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&TypeId::of::<T>()))
            .and_then(|boxed| (boxed as Arc<dyn Any + 'static>).downcast_ref().cloned())
    }

    /// Clear the `State` of all inserted extensions.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// cfg.insert(5i32);
    /// cfg.clear();
    ///
    /// assert!(cfg.get::<i32>().is_none());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        if let Some(ref mut map) = self.map {
            map.clear();
        }
    }

    /// Check whether the extension set is empty or not.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// assert!(cfg.is_empty());
    /// cfg.insert(5i32);
    /// assert!(!cfg.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    /// Get the numer of extensions available.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::State;
    /// let mut cfg = State::new();
    /// assert_eq!(cfg.len(), 0);
    /// cfg.insert(5i32);
    /// assert_eq!(cfg.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    /// 将另一个 State 的内容合并进来（浅拷贝 Arc 值）
    #[inline]
    pub fn extend_from(&mut self, other: &State) {
        if let Some(other_map) = other.map.as_ref() {
            let dst = self.map.get_or_insert_with(Box::default);
            for (k, v) in other_map.iter() {
                dst.insert(*k, v.clone());
            }
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    use tracing::{error, info};

    #[test]
    fn test_type_map() {
        #[derive(Debug, PartialEq, Clone)]
        struct MyType(i32);

        let mut configs = State::new();

        configs.insert(5i32);
        configs.insert(MyType(10));

        assert_eq!(configs.get(), Some(&5i32));
        // assert_eq!(configs.get_mut(), Some(&mut 5i32));

        assert_eq!(configs.remove::<i32>(), Some(5i32));
        assert!(configs.get::<i32>().is_none());

        assert_eq!(configs.get::<bool>(), None);
        assert_eq!(configs.get(), Some(&MyType(10)));

        #[derive(Debug, PartialEq, Clone)]
        struct MyStringType(String);

        configs.insert(MyStringType("Hello".to_string()));

        assert_eq!(
            configs.get::<MyStringType>(),
            Some(&MyStringType("Hello".to_string()))
        );

        use std::thread;
        for i in 0..100 {
            let configs = configs.clone();
            thread::spawn(move || {
                if i % 5 == 0 {
                    // let mut configs = configs.clone();
                    let configs = configs.clone();
                    match configs.get::<MyStringType>() {
                        Some(my_type) => {
                            // my_type.0 = i.to_string();
                            info!("Ok: i:{}, v:{}", i, my_type.0)
                        }
                        _ => {
                            info!("Err: i:{}", i)
                        }
                    }
                } else {
                    match configs.get::<MyStringType>() {
                        Some(my_type) => {
                            info!("Ok: i:{}, v:{}", i, my_type.0)
                        }
                        _ => {
                            info!("Err: i:{}", i)
                        }
                    }
                }
            });
        }
    }

    #[test]
    fn test_type_map_mut_ref() {
        let mut configs = State::default();
        #[derive(Debug, PartialEq, Clone)]
        struct MyStringType(String);

        configs.insert(Arc::new(RwLock::new(MyStringType("Hello".to_string()))));
        assert_eq!(
            configs
                .get::<Arc<RwLock<MyStringType>>>()
                .cloned()
                .unwrap()
                .read()
                .unwrap()
                .0
                .clone(),
            "Hello"
        );

        use std::thread;
        for i in 0..100 {
            let configs = configs.clone();
            thread::spawn(move || {
                if i % 5 == 0 {
                    let configs = configs.clone();
                    match configs.get::<Arc<RwLock<MyStringType>>>().cloned() {
                        Some(my_type) => match my_type.write() {
                            Ok(mut my_type) => {
                                my_type.0 = i.to_string();
                                info!("Ok: i:{}, v:{}", i, my_type.0)
                            }
                            _ => {
                                error!("Rwlock Lock Err: i:{}", i)
                            }
                        },
                        _ => {
                            error!("Get Err: i:{}", i)
                        }
                    }
                } else {
                    match configs.get::<Arc<RwLock<MyStringType>>>() {
                        Some(my_type) => match my_type.read() {
                            Ok(my_type) => {
                                info!("Ok: i:{}, v:{}", i, my_type.0)
                            }
                            _ => {
                                error!("Rwlock Read Err: i:{}", i)
                            }
                        },
                        _ => {
                            error!("Err: i:{}", i)
                        }
                    }
                }
            });
        }
    }
}
