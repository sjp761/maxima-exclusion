use futures::{StreamExt, Future, stream};

pub async fn execute_batch_concurrent<T, F, Fut, R>(buffer: usize, items: Vec<T>, closure: F) -> Vec<R>
where
    T: Send + 'static,
    F: Copy + Fn(T) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let tasks = stream::iter(items).map(|item| async move {
        closure(item).await
    }).buffered(buffer);

    tasks.collect::<Vec<_>>().await
}