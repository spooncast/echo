use {
    crate::{
        message::{
            event_channel ,  message_channel ,  EventAccepter ,  EventResponder ,  MessageRequester , 
            SessionMessage ,  SessionState , 
        } , 
        session::EchoSession , 
        Error , 
    } , 
    actix_cors::Cors , 
    actix_session::{CookieSession ,  Session} , 
    actix_web::{
        client::Client , 
        http::header::{self ,  Header} , 
        middleware ,  web ,  App ,  HttpRequest ,  HttpResponse ,  HttpServer , 
    } , 
    actix_web_httpauth::headers::authorization::{Authorization ,  Basic ,  Bearer} , 
    anyhow::Result , 
    echo_core::{
        authorization::{Authorization as EchoAuthorization ,  Error as AuthError} , 
        session::{
            AppName ,  IdGenerator ,  ManageMessage ,  ManagerHandle ,  SessionId ,  SessionProps , 
            StateReason ,  SPROP_CLIENT_IP , 
        } , 
        Config , 
    } , 
    echo_types::{MediaFormat ,  Protocol} , 
    public_ip::{dns ,  http ,  BoxToResolver ,  ToResolver} , 
    serde::{Deserialize ,  Serialize} , 
    std::{
        collections::{HashMap ,  VecDeque} , 
        net::IpAddr , 
        sync::Arc , 
    } , 
    tokio::sync::{oneshot ,  RwLock} , 
};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const TEST_VERSION: u8 = 0;
static OPTION_V4: &'static str =
    // r###"{"versions":[3 , 4] , "protocols":["srt" , "rtmp"] , "formats":["aac"]}"###; // XXX When the SDK bug is fixed , 
    r###"{"versions":[1 , 2 , 3 , 4] , "protocols":["srt" , "rtmp"] , "formats":["aac"]}"###; // XXX SDK bug

async fn option(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
        .body(OPTION_V4)
}

#[derive(Debug ,  Clone ,  Serialize ,  Deserialize)]
pub struct Transport {
    #[serde(rename = "type")]
    pub addr_type: String , 
    pub address: String , 
    pub port: u16 , 
}

#[derive(Debug ,  Clone ,  Copy ,  Serialize ,  Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Audio , 
}

#[derive(Debug ,  Clone ,  Serialize ,  Deserialize)]
pub struct Media {
    #[serde(rename = "type")]
    pub media_type: MediaType , 
    pub protocol: Protocol , 
    pub format: MediaFormat , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct PublishRequestV3 {
    media: Media , 
    props: Option<SessionProps> , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct PublishRequestV4 {
    media: Media , 
    reason: StateReason , 
    props: Option<SessionProps> , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct RtmpTransport {
    url: String , 
    name: AppName , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
enum PublishResponse {
    #[serde(rename = "publish")]
    V3 {
        name: AppName , 
        transports: Vec<Transport> , 
        media: Media , 
        rtmp: RtmpTransport , 
    } , 
    #[serde(rename = "publish")]
    V4 {
        name: AppName , 
        control: String , 
        transports: Vec<Transport> , 
        media: Media , 
        rtmp: RtmpTransport , 
    } , 
}

#[derive(Debug ,  Deserialize)]
struct CommonRequestV4 {
    reason: StateReason , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct CommonResult {
    name: AppName , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct TeardownResponse {
    teardown: CommonResult , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct PauseResponse {
    pause: CommonResult , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct ResumeResponse {
    resume: CommonResult , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
struct StateResult {
    name: AppName , 
    state: SessionState , 
}

#[derive(Debug ,  Serialize ,  Deserialize)]
#[serde(rename = "teardown")]
struct StateResponse {
    state: StateResult , 
}

pub(crate) struct EchoSessionAvatar {
    name: AppName , 
    port: u16 , 
    state: SessionState , 
    requester: MessageRequester , 
}

static SESSION_ID: &'static str = "id";
static SESSION_NAME: &'static str = "name";
static SERVER_ADDR: &'static str = "addr";

fn set_session(
    session: Session , 
    id: SessionId , 
    name: &str , 
    addr: IpAddr , 
) -> Result<() ,  actix_web::Error> {
    session.set(SESSION_ID ,  id)?;
    session.set(SESSION_NAME ,  name.to_string())?;
    session.set(SERVER_ADDR ,  addr)?;
    Ok(())
}

fn get_session(session: Session) -> Result<Option<(SessionId ,  AppName ,  IpAddr)> ,  actix_web::Error> {
    let id = match session.get::<SessionId>(SESSION_ID) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => return Ok(None) , 
        } , 
        Err(err) => return Err(err) , 
    };
    let name = match session.get::<AppName>(SESSION_NAME) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => return Ok(None) , 
        } , 
        Err(err) => return Err(err) , 
    };
    let addr = match session.get::<IpAddr>(SERVER_ADDR) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => return Ok(None) , 
        } , 
        Err(err) => return Err(err) , 
    };

    Ok(Some((id ,  name ,  addr)))
}

async fn authorize(
    req: &HttpRequest , 
    name: String , 
    service: Arc<ServiceInner> , 
) -> Result<String ,  Error> {
    match Authorization::<Bearer>::parse(req) {
        Ok(auth) => {
            let token = auth.as_ref().token();
            log::info!("{} \"bearer {}\"" ,  name ,  token);

            let (authenticator ,  responder) = oneshot::channel();
            let auth = EchoAuthorization::Bearer(token.to_string());
            service
                .session_manager
                .send(ManageMessage::AuthorizeSession(
                    name.to_string() , 
                    auth , 
                    authenticator , 
                ))
                .map_err(|_| Error::SessionCreationFailed)?;
            match responder.await.unwrap() {
                Ok(key) => {
                    return Ok(key);
                }
                Err(err) => {
                    log::warn!("{} \"bearer {}\"" ,  name ,  err);
                    return Err(Error::Unauthorized);
                }
            }
        }
        Err(_) => match Authorization::<Basic>::parse(req) {
            Ok(auth) => {
                let user_id = auth.as_ref().user_id();
                let password = match auth.as_ref().password() {
                    Some(p) => p.as_ref() , 
                    None => "" , 
                };
                log::info!("{} \"basic {}:{}\"" ,  name ,  user_id ,  password);

                let (authenticator ,  responder) = oneshot::channel();
                let auth = EchoAuthorization::Basic(user_id.to_string() ,  password.to_string());
                service
                    .session_manager
                    .send(ManageMessage::AuthorizeSession(
                        name.to_string() , 
                        auth , 
                        authenticator , 
                    ))
                    .map_err(|_| Error::SessionCreationFailed)?;

                match responder.await.unwrap() {
                    Ok(key) => {
                        return Ok(key);
                    }
                    Err(AuthError::ExpiredToken) => {
                        log::warn!("{} \"bearer expired token\"" ,  name);
                        return Err(Error::ExpiredToken);
                    }
                    Err(err) => {
                        log::warn!("{} \"bearer {}\"" ,  name ,  err);
                        return Err(Error::Unauthorized);
                    }
                }
            }
            Err(err) => {
                log::error!("{} \"auth {}\"" ,  name ,  err);
                return Err(Error::Unauthorized);
            }
        } , 
    }
}

fn publish_response(
    ver: u8 , 
    name: &str , 
    media: &Media , 
    port: u16 , 
    key: String , 
    service: Arc<ServiceInner> , 
) -> Result<PublishResponse ,  Error> {
    let conn_ip = if let Some(pub_ip) = service.config.echo_srt_pub_ip {
        pub_ip
    } else if let Some(pub_ip) = &service.public_ip {
        *pub_ip
    } else {
        service.config.echo_srt_priv_ip
    };

    let transports = match media.protocol {
        Protocol::SRT => vec![Transport {
            addr_type: "IPv4".to_string() , 
            address: conn_ip.to_string() , 
            port , 
        }] , 
        _ => vec![] , 
    };

    if ver == 3 {
        Ok(PublishResponse::V3 {
            name: name.to_string() , 
            transports , 
            media: media.clone() , 
            rtmp: RtmpTransport {
                url: format!("rtmp://{}/{}" ,  conn_ip ,  name) , 
                name: key , 
            } , 
        })
    } else {
        Ok(PublishResponse::V4 {
            name: name.to_string() , 
            control: format!("http://{}:5021/echo/{}" ,  conn_ip ,  ver) , 
            transports , 
            media: media.clone() , 
            rtmp: RtmpTransport {
                url: format!("rtmp://{}/{}" ,  conn_ip ,  name) , 
                name: key , 
            } , 
        })
    }
}

async fn publish_test(
    path: web::Path<String> , 
    pub_req: web::Json<PublishRequestV4> , 
    _http_req: HttpRequest , 
    _session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let service = service.as_ref();
    let name = path.into_inner();
    let port = service.config.echo_srt_min_port;

    match publish_response(
        TEST_VERSION , 
        &name , 
        &pub_req.0.media , 
        port , 
        "key".to_string() , 
        service.clone() , 
    ) {
        Ok(res) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
            .json(res)) , 
        Err(err) => Err(err) , 
    }
}

async fn publish(
    ver: u8 , 
    name: String , 
    pub_req: PublishRequestV4 , 
    http_req: HttpRequest , 
    session: Session , 
    service: Arc<ServiceInner> , 
) -> Result<HttpResponse ,  Error> {
    log::info!("{} publish request {:?}" ,  name ,  pub_req);
    let key = authorize(&http_req ,  name.clone() ,  service.clone()).await?;
    if let Some(mut props) = pub_req.props {
        props.insert(
            SPROP_CLIENT_IP.to_string() , 
            http_req.peer_addr().unwrap().ip().to_string() , 
        );
        service
            .session_manager
            .send(ManageMessage::UpdateSessionProps(name.clone() ,  props))
            .map_err(|_| Error::SessionCreationFailed)?;
    }

    let mut port = 0;
    match pub_req.media.protocol {
        Protocol::SRT => {
            let session_id = service.id_gen.fetch_next();
            port = {
                let mut ports = service.ports.write().await;
                if ports.is_empty() {
                    log::error!("{} publish error - no available port" ,  name , );
                    return Err(Error::OtherStr("no available port"));
                }
                ports.pop_front().unwrap()
            };

            let responder = service.responder.clone();
            let (requester ,  accepter) = message_channel();
            let echo_session = EchoSession::new(
                session_id , 
                &name , 
                port , 
                service.config.echo_srt_connection_timeout , 
                service.config.echo_srt_read_timeout , 
                service.config.echo_srt_latency , 
                accepter , 
                responder , 
                service.session_manager.clone() , 
                pub_req.reason , 
            );
            let session_avatar = EchoSessionAvatar {
                name: name.to_string() , 
                port , 
                state: SessionState::Init , 
                requester , 
            };

            {
                let mut sessions = service.sessions.write().await;
                sessions.insert(session_id ,  session_avatar);

                if let Err(err) =
                    set_session(session ,  session_id ,  &name ,  service.config.echo_srt_priv_ip)
                {
                    let mut ports = service.ports.write().await;
                    ports.push_back(port);
                    log::error!("{} publish error - {}\"" ,  name ,  err);
                    return Err(Error::OtherString(err.to_string()));
                }
            }

            tokio::spawn(echo_session.run());
        }
        _ => {}
    }

    match publish_response(ver ,  &name ,  &pub_req.media ,  port ,  key ,  service.clone()) {
        Ok(res) => Ok(HttpResponse::Ok()
            .content_type("application/json")
            .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
            .json(res)) , 
        Err(err) => Err(err) , 
    }
}

async fn publish_v3(
    path: web::Path<String> , 
    pub_req: web::Json<PublishRequestV3> , 
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let name = path.into_inner();
    let pub_req_v3 = pub_req.into_inner();
    let pub_req = PublishRequestV4 {
        media: pub_req_v3.media , 
        reason: StateReason::unknown() , 
        props: pub_req_v3.props , 
    };
    let service = service.get_ref().clone();
    publish(3 ,  name ,  pub_req ,  http_req ,  session ,  service).await
}

async fn publish_v4(
    path: web::Path<String> , 
    pub_req: web::Json<PublishRequestV4> , 
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let name = path.into_inner();
    let pub_req = pub_req.into_inner();
    let service = service.get_ref().clone();
    publish(4 ,  name ,  pub_req ,  http_req ,  session ,  service).await
}

async fn forward(
    req: &HttpRequest , 
    server_ip: IpAddr , 
    def_port: u16 , 
    body: web::Bytes , 
) -> Result<HttpResponse ,  Error> {
    let forward_url = format!(
        "http://{}:{}{}" , 
        server_ip , 
        req.uri().port_u16().unwrap_or(def_port) , 
        req.uri().path()
    );

    let client = Client::new();
    let forwarded_req = client.request_from(forward_url ,  req.head()).no_decompress();
    let forwarded_req = if let Some(addr) = req.head().peer_addr {
        forwarded_req.header("x-forwarded-for" ,  format!("{}" ,  addr.ip()))
    } else {
        forwarded_req
    };

    let mut res = forwarded_req
        .send_body(body)
        .await
        .map_err(|_| Error::SessionForwardFailed)?;

    let mut client_resp = HttpResponse::build(res.status());
    // Remove `Connection` as per
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
    for (header_name ,  header_value) in res.headers().iter().filter(|(h ,  _)| *h != "connection") {
        client_resp.header(header_name.clone() ,  header_value.clone());
    }

    Ok(client_resp.body(res.body().await.map_err(|_| Error::SessionForwardFailed)?))
}

async fn teardown_test(
    _req: web::Json<CommonRequestV4> , 
    _http_req: HttpRequest , 
    _session: Session , 
    _service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let name = String::from("echotester");

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
        .json(TeardownResponse {
            teardown: CommonResult { name } , 
        }))
}

async fn teardown(
    req: CommonRequestV4 , 
    http_req: HttpRequest , 
    session: Session , 
    service: Arc<ServiceInner> , 
) -> Result<HttpResponse ,  Error> {
    let (session_id ,  session_name ,  server_addr) = match get_session(session) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => {
                log::error!("??? teardown error - session id not found");
                return Err(Error::InvalidSessionCookie);
            }
        } , 
        Err(err) => {
            log::error!("??? teardown error - {}" ,  err);
            return Err(Error::InvalidSessionCookie);
        }
    };

    if service.config.echo_srt_priv_ip == server_addr {
        let mut sessions = service.sessions.write().await;
        if let Some(ref mut session_avatar) = sessions.get_mut(&session_id) {
            let name = session_avatar.name.to_string();
            if let Err(_) = session_avatar
                .requester
                .send(SessionMessage::Shutdown(req.reason))
            {
                log::error!("{} teardown error - session shutdown send error" ,  name);
            }

            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
                .json(TeardownResponse {
                    teardown: CommonResult { name } , 
                }))
        } else {
            // using RTMP still request srt teardown.
            log::warn!("{} teardown error - session not found" ,  session_name);
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
                .json(TeardownResponse {
                    teardown: CommonResult { name: session_name } , 
                }))
            // Err(Error::SessionNotFound(session_id ,  session_name))
        }
    } else {
        match forward(
            &http_req , 
            server_addr , 
            service.config.echo_addr.port() , 
            web::Bytes::default() , 
        )
        .await
        {
            Ok(resp) => Ok(resp) , 
            Err(err) => {
                log::error!(
                    "{} teardown error - session forward error: {}" , 
                    session_name , 
                    err
                );
                Err(err)
            }
        }
    }
}

async fn teardown_v3(
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let req = CommonRequestV4 {
        reason: StateReason::unknown() , 
    };
    let service = service.get_ref().clone();
    teardown(req ,  http_req ,  session ,  service).await
}

async fn teardown_v4(
    req: web::Json<CommonRequestV4> , 
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let req = req.into_inner();
    let service = service.get_ref().clone();
    teardown(req ,  http_req ,  session ,  service).await
}

async fn pause_v4(
    req: web::Json<CommonRequestV4> , 
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let (session_id ,  session_name ,  server_addr) = match get_session(session) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => {
                log::error!("??? pause error - session id not found");
                return Err(Error::InvalidSessionCookie);
            }
        } , 
        Err(err) => {
            log::error!("??? pause error - {}" ,  err);
            return Err(Error::InvalidSessionCookie);
        }
    };

    if service.config.echo_srt_priv_ip == server_addr {
        let mut sessions = service.sessions.write().await;
        if let Some(ref mut session_avatar) = sessions.get_mut(&session_id) {
            let name = session_avatar.name.to_string();
            if let Err(_) = session_avatar
                .requester
                .send(SessionMessage::Pause(req.into_inner().reason))
            {
                log::error!("{} pause error - session shutdown send error" ,  name);
            }

            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
                .json(PauseResponse {
                    pause: CommonResult { name } , 
                }))
        } else {
            log::error!("{} pause error - session not found" ,  session_name);
            Err(Error::SessionNotFound(session_id ,  session_name))
        }
    } else {
        match forward(
            &http_req , 
            server_addr , 
            service.config.echo_addr.port() , 
            web::Bytes::default() , 
        )
        .await
        {
            Ok(resp) => Ok(resp) , 
            Err(err) => {
                log::error!(
                    "{} pause error - session forward error: {}" , 
                    session_name , 
                    err
                );
                Err(err)
            }
        }
    }
}

async fn resume_v4(
    req: web::Json<CommonRequestV4> , 
    http_req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let (session_id ,  session_name ,  server_addr) = match get_session(session) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => {
                log::error!("??? resume error - session id not found");
                return Err(Error::InvalidSessionCookie);
            }
        } , 
        Err(err) => {
            log::error!("??? resume error - {}" ,  err);
            return Err(Error::InvalidSessionCookie);
        }
    };

    if service.config.echo_srt_priv_ip == server_addr {
        let mut sessions = service.sessions.write().await;
        if let Some(ref mut session_avatar) = sessions.get_mut(&session_id) {
            let name = session_avatar.name.to_string();
            if let Err(_) = session_avatar
                .requester
                .send(SessionMessage::Resume(req.into_inner().reason))
            {
                log::error!("{} resume error - session shutdown send error" ,  name);
            }

            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
                .json(ResumeResponse {
                    resume: CommonResult { name } , 
                }))
        } else {
            log::error!("{} resume error - session not found" ,  session_name);
            Err(Error::SessionNotFound(session_id ,  session_name))
        }
    } else {
        match forward(
            &http_req , 
            server_addr , 
            service.config.echo_addr.port() , 
            web::Bytes::default() , 
        )
        .await
        {
            Ok(resp) => Ok(resp) , 
            Err(err) => {
                log::error!(
                    "{} resume error - session forward error: {}" , 
                    session_name , 
                    err
                );
                Err(err)
            }
        }
    }
}

async fn state_test(
    _req: HttpRequest , 
    _session: Session , 
    _service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let name = String::from("echotester");
    let state = SessionState::Ready;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
        .json(StateResponse {
            state: StateResult { name ,  state } , 
        }))
}

async fn state_v4(
    req: HttpRequest , 
    session: Session , 
    service: web::Data<Arc<ServiceInner>> , 
) -> Result<HttpResponse ,  Error> {
    let (session_id ,  session_name ,  server_addr) = match get_session(session) {
        Ok(opt) => match opt {
            Some(v) => v , 
            None => {
                log::error!("??? state error - session id not found");
                return Err(Error::InvalidSessionCookie);
            }
        } , 
        Err(err) => {
            log::error!("??? state error - {}" ,  err);
            return Err(Error::InvalidSessionCookie);
        }
    };

    if service.config.echo_srt_priv_ip == server_addr {
        let sessions = service.sessions.read().await;
        if let Some(ref session_avatar) = sessions.get(&session_id) {
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .header(header::SERVER ,  format!("Echo/{}" ,  VERSION.unwrap()))
                .json(StateResponse {
                    state: StateResult {
                        name: session_avatar.name.to_string() , 
                        state: session_avatar.state , 
                    } , 
                }))
        } else {
            log::error!("{} state error - session not found" ,  session_name);
            Err(Error::SessionNotFound(session_id ,  session_name))
        }
    } else {
        match forward(
            &req , 
            server_addr , 
            service.config.echo_addr.port() , 
            web::Bytes::default() , 
        )
        .await
        {
            Ok(resp) => Ok(resp) , 
            Err(err) => {
                log::error!(
                    "{} state error - session forward error: {}" , 
                    session_name , 
                    err
                );
                Err(err)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ServiceInner {
    pub config: Config , 
    pub session_manager: ManagerHandle , 
    pub id_gen: IdGenerator , 
    public_ip: Option<IpAddr> , 
    sessions: Arc<RwLock<HashMap<SessionId ,  EchoSessionAvatar>>> , 
    ports: Arc<RwLock<VecDeque<u16>>> , 
    responder: EventResponder , 
}

pub struct Service {
    config: Config , 
    session_manager: ManagerHandle , 
    id_gen: IdGenerator , 
}

async fn background_service(service: Arc<ServiceInner> ,  mut accepter: EventAccepter) -> Result<()> {
    while let Some(event) = accepter.recv().await {
        let session_id = event.id;
        let session_name = event.name;
        let mut sessions = service.sessions.write().await;
        match event.state {
            SessionState::Terminated => {
                if let Some(ref mut session_avatar) = sessions.remove(&session_id) {
                    log::info!("{} session terminated" ,  session_name);
                    let mut ports = service.ports.write().await;
                    ports.push_back(session_avatar.port);
                }
            }
            state => {
                if let Some(ref mut session_avatar) = sessions.get_mut(&session_id) {
                    log::info!("{} session state changed {:?}" ,  session_name ,  state);
                    session_avatar.state = state;
                }
            }
        }
    }
    Ok(())
}

impl Service {
    pub fn new(session_manager: ManagerHandle ,  config: Config ,  id_gen: IdGenerator) -> Self {
        Self {
            config , 
            session_manager , 
            id_gen , 
        }
    }

    pub async fn run(self) -> Result<()> {
        let public_ip = public_ip().await;
        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let ports = Arc::new(RwLock::new(
            (self.config.echo_srt_min_port..=self.config.echo_srt_max_port).collect() , 
        ));
        let (responder ,  accepter) = event_channel();

        let service = Arc::new(ServiceInner {
            config: self.config.clone() , 
            session_manager: self.session_manager.clone() , 
            id_gen: self.id_gen.clone() , 
            public_ip , 
            sessions , 
            ports , 
            responder , 
        });

        let _bg_svc = tokio::spawn(background_service(service.clone() ,  accepter));

        let listen = self.config.echo_addr;
        HttpServer::new(move || {
            let service = service.clone();
            let mut private_key = service.config.echo_priv_key.as_bytes().to_vec();
            private_key.resize_with(32 ,  Default::default);
            App::new()
                .wrap(
                    CookieSession::signed(&private_key)
                        .name("echo")
                        .secure(false) , 
                )
                .wrap(Cors::permissive()) // XXX for quick development
                .wrap(middleware::Logger::default())
                .data(web::JsonConfig::default().limit(4096))
                .data(service)
                .service(web::resource("/echo/option").to(option))
                // for testing
                .service(
                    web::resource("/echo/0/publish/{name}").route(web::post().to(publish_test)) , 
                )
                // TODO pause ,  resume
                .service(web::resource("/echo/0/teardown").route(web::put().to(teardown_test)))
                .service(web::resource("/echo/0/state").to(state_test))
                // v3 current version
                .service(web::resource("/echo/3/publish/{name}").route(web::post().to(publish_v3)))
                .service(web::resource("/echo/3/teardown").route(web::post().to(teardown_v3)))
                // v4 not yet
                .service(web::resource("/echo/4/publish/{name}").route(web::post().to(publish_v4)))
                .service(web::resource("/echo/4/pause").route(web::put().to(pause_v4)))
                .service(web::resource("/echo/4/resume").route(web::put().to(resume_v4)))
                .service(web::resource("/echo/4/teardown").route(web::put().to(teardown_v4)))
                .service(web::resource("/echo/4/state").to(state_v4))
        })
        .bind(listen)?
        .run()
        .await?;

        // XXX graceful shutdown

        Ok(())
    }
}

async fn public_ip() -> Option<IpAddr> {
    // List of resolvers to try and get an IP address from
    let resolver = vec![
        BoxToResolver::new(dns::OPENDNS_RESOLVER) , 
        BoxToResolver::new(http::HTTP_IPIFY_ORG_RESOLVER) , 
    ]
    .to_resolver();
    public_ip::resolve_address(resolver).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::dev::Service;
    use actix_web::{
        http::{header ,  StatusCode} , 
        test ,  web ,  App ,  Error , 
    };
    use echo_core::session::{IdGenerator ,  SessionManager};

    #[actix_rt::test]
    async fn test_option() -> Result<() ,  Error> {
        let mut app =
            test::init_service(App::new().service(web::resource("/echo/option").to(option))).await;

        let req = test::TestRequest::get().uri("/echo/option").to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status() ,  StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::SERVER)
                .unwrap()
                .to_str()
                .unwrap() , 
            format!("Echo/{}" ,  VERSION.unwrap())
        );

        let resp_body = match resp.response().body().as_ref() {
            Some(actix_web::body::Body::Bytes(bytes)) => bytes , 
            _ => panic!("Response error") , 
        };

        assert_eq!(resp_body ,  OPTION_V4);

        Ok(())
    }

    #[actix_rt::test]
    async fn test_publish() -> Result<() ,  Error> {
        let config = Config::default();

        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let ports = Arc::new(RwLock::new(
            (config.echo_srt_min_port..=config.echo_srt_max_port).collect() , 
        ));
        let (responder ,  _accepter) = event_channel();

        let session_manager = SessionManager::new(config.clone());
        let manager_handle = session_manager.handle();
        let id_gen = IdGenerator::new();

        let service = Arc::new(ServiceInner {
            config: config.clone() , 
            session_manager: manager_handle , 
            id_gen , 
            public_ip: None , 
            sessions , 
            ports , 
            responder , 
        });

        let mut app =
            test::init_service(App::new().data(service).service(
                web::resource("/echo/0/publish/{name}").route(web::post().to(publish_test)) , 
            ))
            .await;

        let req = test::TestRequest::post()
            .uri("/echo/0/publish/echotester")
            .header(header::ACCEPT ,  "application/json")
            .header(header::CONTENT_TYPE ,  "application/json")
            .header(header::AUTHORIZATION ,  "Bearer hahaha")
            .header(
                header::USER_AGENT , 
                "Spoon/5.4.0 EchoLiveKit/1.2.7 (iPhone OS 10_3; iPhone11 , 4)" , 
            )
            .set_payload(r###"{"media":{"type":"audio" , "protocol":"srt" , "format":"aac"} , "reason":{"code":50000 , "message":"unknown"} , "props":{}}"###)
            .to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status() ,  StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::SERVER)
                .unwrap()
                .to_str()
                .unwrap() , 
            format!("Echo/{}" ,  VERSION.unwrap())
        );

        let resp_body = match resp.response().body().as_ref() {
            Some(actix_web::body::Body::Bytes(bytes)) => bytes , 
            _ => panic!("Response error") , 
        };

        assert_eq!(
            resp_body , 
            r###"{"publish":{"name":"echotester" , "control":"http://127.0.0.1:5021/echo/0" , "transports":[{"type":"IPv4" , "address":"127.0.0.1" , "port":30000}] , "media":{"type":"audio" , "protocol":"srt" , "format":"aac"} , "rtmp":{"url":"rtmp://127.0.0.1/echotester" , "name":"key"}}}"###
        );

        Ok(())
    }

    #[actix_rt::test]
    async fn test_publish_rtmp() -> Result<() ,  Error> {
        let config = Config::default();

        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let ports = Arc::new(RwLock::new(
            (config.echo_srt_min_port..=config.echo_srt_max_port).collect() , 
        ));
        let (responder ,  _accepter) = event_channel();

        let session_manager = SessionManager::new(config.clone());
        let manager_handle = session_manager.handle();
        let id_gen = IdGenerator::new();

        let service = Arc::new(ServiceInner {
            config: config.clone() , 
            session_manager: manager_handle , 
            id_gen , 
            public_ip: None , 
            sessions , 
            ports , 
            responder , 
        });

        let mut app =
            test::init_service(App::new().data(service).service(
                web::resource("/echo/0/publish/{name}").route(web::post().to(publish_test)) , 
            ))
            .await;

        let req = test::TestRequest::post()
            .uri("/echo/0/publish/echotester")
            .header(header::ACCEPT ,  "application/json")
            .header(header::CONTENT_TYPE ,  "application/json")
            .header(header::AUTHORIZATION ,  "Bearer hahaha")
            .header(
                header::USER_AGENT , 
                "Spoon/5.4.0 EchoLiveKit/1.2.7 (iPhone OS 10_3; iPhone11 , 4)" , 
            )
            .set_payload(r###"{"media":{"type":"audio" , "protocol":"rtmp" , "format":"flv"} , "reason":{"code":50000 , "message":"unknown"} , "props":{}}"###)
            .to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status() ,  StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::SERVER)
                .unwrap()
                .to_str()
                .unwrap() , 
            format!("Echo/{}" ,  VERSION.unwrap())
        );

        let resp_body = match resp.response().body().as_ref() {
            Some(actix_web::body::Body::Bytes(bytes)) => bytes , 
            _ => panic!("Response error") , 
        };

        assert_eq!(
            resp_body , 
            r###"{"publish":{"name":"echotester" , "control":"http://127.0.0.1:5021/echo/0" , "transports":[] , "media":{"type":"audio" , "protocol":"rtmp" , "format":"flv"} , "rtmp":{"url":"rtmp://127.0.0.1/echotester" , "name":"key"}}}"###
        );

        Ok(())
    }

    #[actix_rt::test]
    async fn test_teardown() -> Result<() ,  Error> {
        let config = Config::default();

        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let ports = Arc::new(RwLock::new(
            (config.echo_srt_min_port..=config.echo_srt_max_port).collect() , 
        ));
        let (responder ,  _accepter) = event_channel();

        let session_manager = SessionManager::new(config.clone());
        let manager_handle = session_manager.handle();
        let id_gen = IdGenerator::new();

        let service = Arc::new(ServiceInner {
            config: config.clone() , 
            session_manager: manager_handle , 
            id_gen , 
            public_ip: None , 
            sessions , 
            ports , 
            responder , 
        });

        let mut app = test::init_service(
            App::new()
                .data(service)
                .service(web::resource("/echo/0/teardown").route(web::post().to(teardown_test))) , 
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/echo/0/teardown")
            .header(header::ACCEPT ,  "application/json")
            .header(header::CONTENT_TYPE ,  "application/json")
            .set_payload(r###"{"reason":{"code":50000 , "message":"unknown"}}"###)
            .to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status() ,  StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::SERVER)
                .unwrap()
                .to_str()
                .unwrap() , 
            format!("Echo/{}" ,  VERSION.unwrap())
        );

        let resp_body = match resp.response().body().as_ref() {
            Some(actix_web::body::Body::Bytes(bytes)) => bytes , 
            _ => panic!("Response error") , 
        };

        assert_eq!(resp_body ,  r###"{"teardown":{"name":"echotester"}}"###);

        Ok(())
    }

    #[actix_rt::test]
    async fn test_state() -> Result<() ,  Error> {
        let config = Config::default();

        let sessions = Arc::new(RwLock::new(HashMap::new()));
        let ports = Arc::new(RwLock::new(
            (config.echo_srt_min_port..=config.echo_srt_max_port).collect() , 
        ));
        let (responder ,  _accepter) = event_channel();

        let session_manager = SessionManager::new(config.clone());
        let manager_handle = session_manager.handle();
        let id_gen = IdGenerator::new();

        let service = Arc::new(ServiceInner {
            config: config.clone() , 
            session_manager: manager_handle , 
            id_gen , 
            public_ip: None , 
            sessions , 
            ports , 
            responder , 
        });

        let mut app = test::init_service(
            App::new()
                .data(service)
                .service(web::resource("/echo/0/state").to(state_test)) , 
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/echo/0/state")
            .header(header::ACCEPT ,  "application/json")
            .header(header::CONTENT_TYPE ,  "application/json")
            .to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status() ,  StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::SERVER)
                .unwrap()
                .to_str()
                .unwrap() , 
            format!("Echo/{}" ,  VERSION.unwrap())
        );

        let resp_body = match resp.response().body().as_ref() {
            Some(actix_web::body::Body::Bytes(bytes)) => bytes , 
            _ => panic!("Response error") , 
        };

        assert_eq!(
            resp_body , 
            r###"{"state":{"name":"echotester" , "state":"ready"}}"###
        );

        Ok(())
    }
}
