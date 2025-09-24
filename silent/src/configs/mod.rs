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

/// A type map of protocol extensions.
///
/// `Configs` can be used by `Request` and `Response` to store
/// extra data derived from the underlying protocol.
#[derive(Default, Clone)]
pub struct Configs {
    // If extensions are never used, no need to carry around an empty HashMap.
    // That's 3 words. Instead, this is only 1 word.
    map: Option<Box<AnyMap>>,
}

impl Configs {
    /// Create an empty `Configs`.
    #[inline]
    pub fn new() -> Configs {
        Configs { map: None }
    }

    /// Insert a type into this `Configs`.
    ///
    /// If an extension of this type already existed, it will
    /// be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
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

    /// Get a reference to a type previously inserted on this `Configs`.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
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

    /// Remove a type from this `Configs`.
    ///
    /// If aa extension of this type existed, it will be returned.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
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

    /// Clear the `Configs` of all inserted extensions.
    ///
    /// # Example
    ///
    /// ```
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
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
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
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
    /// # use silent::Configs;
    /// let mut cfg = Configs::new();
    /// assert_eq!(cfg.len(), 0);
    /// cfg.insert(5i32);
    /// assert_eq!(cfg.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    /// 将另一个 Configs 的内容合并进来（浅拷贝 Arc 值）
    #[inline]
    pub fn extend_from(&mut self, other: &Configs) {
        if let Some(other_map) = other.map.as_ref() {
            let dst = self.map.get_or_insert_with(Box::default);
            for (k, v) in other_map.iter() {
                dst.insert(*k, v.clone());
            }
        }
    }
}

impl fmt::Debug for Configs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Configs").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    use tracing::{error, info};

    #[test]
    fn test_configs() {
        #[derive(Debug, PartialEq, Clone)]
        struct MyType(i32);

        let mut configs = Configs::new();

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
    fn test_configs_mut_ref() {
        let mut configs = Configs::default();
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
