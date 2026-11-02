use super::*;
use rolvya_domain::ids::ProjectId;
use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, watch};
struct EventBody {
    receiver: mpsc::Receiver<axum::body::Bytes>,
    authority: Arc<RequestAuthority>,
    context: BrowserContext,
    cancel: watch::Sender<bool>,
    count: Arc<std::sync::atomic::AtomicUsize>,
}
impl Drop for EventBody {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.authority.cancel(&self.context);
        self.count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
struct ProducerLife(Arc<std::sync::atomic::AtomicUsize>);
impl Drop for ProducerLife {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
enum StreamFailure {
    Session,
    Project,
    Gap,
    Unavailable,
}
impl hyper::body::Body for EventBody {
    type Data = axum::body::Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        self.receiver
            .poll_recv(cx)
            .map(|value| value.map(|bytes| Ok(hyper::body::Frame::data(bytes))))
    }
}
impl HumanBrowserServer {
    pub(super) async fn events(
        self: &Arc<Self>,
        h: &SessionHandle,
        project: ProjectId,
        cursor: Option<&str>,
        guard: RequestGuard,
    ) -> Result<Response<Body>, AuthError> {
        let mut after = match self
            .auth
            .scoped_panel_events(h, &project, cursor, &guard.context)
            .await
        {
            Ok((bound, _)) => bound,
            Err(AuthError::Denied) => {
                return Ok(json(
                    StatusCode::FORBIDDEN,
                    &serde_json::json!({"schema":1,"error":"project_access_denied"}),
                ));
            }
            Err(e) => return Err(e),
        };
        let permit = self
            .streams
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Busy)?;
        // Global and network bounds also cover multiple tabs. No bearer is put in a URL.
        let handle = SessionHandle::parse(h.0.to_string())?;
        let context = guard.context.clone();
        let state = self.clone();
        let (sender, receiver) = mpsc::channel(2);
        let (cancel, mut cancelled) = watch::channel(false);
        self.active_producers
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let life = ProducerLife(self.active_producers.clone());
        let body_context = context.clone();
        let mut producers = self.producers.lock().map_err(|_| AuthError::Unavailable)?;
        while producers.try_join_next().is_some() {}
        producers.spawn(async move {
            let _life = life;
            let _permit=permit;
            let _request=guard;
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let started = tokio::time::Instant::now();
            loop {
                tokio::select! {_=cancelled.changed()=>break,_=interval.tick()=>{}}
                if started.elapsed() >= Duration::from_secs(90) {
                    break;
                }
                let requested=after.clone();
                let check=async {
                    state.auth.inspect_session(&handle,&context).await.map_err(|e|if e==AuthError::Denied {StreamFailure::Session}else{StreamFailure::Unavailable})?;
                    state.auth.scoped_panel_events(&handle,&project,Some(&requested),&context).await.map(|(_,events)|events).map_err(|e|match e {AuthError::Denied=>StreamFailure::Project,AuthError::Conflict=>StreamFailure::Gap,_=>StreamFailure::Unavailable})
                };
                tokio::pin!(check);
                let result=tokio::select! {
                    result=&mut check=>result,
                    _=cancelled.changed()=>{
                        // Keep capacity until SQLite acknowledges completion or
                        // rollback. Dropping an async query does not prove its
                        // driver thread has stopped; no new read is started.
                        let _=check.await;break;
                    }
                };
                let events = match result {
                    Ok(events) => events,
                    Err(StreamFailure::Session)=>{let _=sender.try_send(axum::body::Bytes::from_static(b"event: unauthorized\ndata: {\"schema\":1,\"error\":\"unauthorized\"}\n\n"));break;}
                    Err(StreamFailure::Project) => {
                        let _ = sender.try_send(axum::body::Bytes::from_static(
                            b"event: forbidden\ndata: {\"schema\":1,\"error\":\"forbidden\"}\n\n",
                        ));
                        break;
                    }
                    Err(StreamFailure::Gap) => {
                        let _ = sender.try_send(axum::body::Bytes::from_static(
                            b"event: reset\ndata: {\"schema\":1,\"error\":\"replay_gap\"}\n\n",
                        ));
                        break;
                    }
                    Err(_) => break,
                };
                if events.is_empty()
                    && sender
                        .try_send(axum::body::Bytes::from_static(b": keepalive\n\n"))
                        .is_err()
                {
                        break;
                }
                for (number,id) in events {
                    let data = serde_json::json!({"schema":2,"project":project.as_str(),"cursor":number,"revision":number});
                    if sender
                        .try_send(axum::body::Bytes::from(format!(
                            "id: {id}\ndata: {data}\n\n"
                        )))
                        .is_err()
                    {
                        return;
                    }
                    after = id;
                }
            }
        });
        self.open_bodies
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut response = Response::new(Body::new(EventBody {
            receiver,
            authority: self.authority.clone(),
            context: body_context,
            cancel,
            count: self.open_bodies.clone(),
        }));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        response
            .headers_mut()
            .insert("x-accel-buffering", header::HeaderValue::from_static("no"));
        Ok(response)
    }
}
