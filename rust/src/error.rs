pub trait Error: 'static + std::error::Error + Send + Sync {}

impl<T: 'static + std::error::Error + Send + Sync> Error for T {}
