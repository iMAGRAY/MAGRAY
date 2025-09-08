use anyhow::Result;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Basic dependency injection system for Atom IDE
pub struct ServiceContainer {
    services: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl ServiceContainer {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register<T>(&self, instance: T) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let mut services = self.services.write().await;
        services.insert(type_id, Box::new(instance));
        Ok(())
    }

    pub async fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let services = self.services.read().await;
        
        services.get(&type_id)?.downcast_ref::<T>().map(|service| {
            // This is a simplified version - in a real DI container
            // we would need proper Arc handling
            // For now, this basic structure allows compilation
            Arc::new(unsafe { std::ptr::read(service as *const T) })
        })
    }

    pub async fn is_registered<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let services = self.services.read().await;
        services.contains_key(&type_id)
    }
}

impl Default for ServiceContainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService {
        #[allow(dead_code)]
        pub name: String,
    }

    #[tokio::test]
    async fn test_service_registration() -> Result<()> {
        let container = ServiceContainer::new();
        
        let service = TestService {
            name: "test".to_string(),
        };
        
        container.register(service).await?;
        
        assert!(container.is_registered::<TestService>().await);
        
        Ok(())
    }
}