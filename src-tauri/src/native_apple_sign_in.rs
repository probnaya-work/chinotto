//! macOS Sign in with Apple via AuthenticationServices → JWT for Firebase.

use std::cell::RefCell;
use std::sync::mpsc::SyncSender;
use std::sync::Mutex;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::NSWindow;
use objc2_authentication_services::{
    ASAuthorization, ASAuthorizationAppleIDCredential, ASAuthorizationAppleIDProvider,
    ASAuthorizationController, ASAuthorizationControllerDelegate, ASAuthorizationError,
    ASAuthorizationControllerPresentationContextProviding, ASAuthorizationOperationLogin,
    ASAuthorizationRequest, ASAuthorizationScopeEmail, ASAuthorizationScopeFullName,
    ASPresentationAnchor,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSError, NSObject, NSObjectProtocol, NSString,
};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri::Manager;

use crate::NativeAppleSignInResult;

const APPLE_SIGN_IN_CANCELLED: &str = "APPLE_SIGN_IN_CANCELLED";

struct SignInKeepAlive {
    delegate: Retained<AppleAuthDelegate>,
    controller: Retained<ASAuthorizationController>,
}

thread_local! {
    static SIGN_IN_KEEP_ALIVE: RefCell<Option<SignInKeepAlive>> = const { RefCell::new(None) };
}

fn clear_keep_alive() {
    SIGN_IN_KEEP_ALIVE.with(|c| *c.borrow_mut() = None);
}

fn send_result(ivars: &AppleAuthIvars, result: Result<NativeAppleSignInResult, String>) {
    let mut tx = ivars.done_tx.lock().expect("AppleAuthIvars.done_tx poisoned");
    if let Some(sender) = tx.take() {
        let _ = sender.send(result);
    }
    clear_keep_alive();
}

struct AppleAuthIvars {
    done_tx: Mutex<Option<SyncSender<Result<NativeAppleSignInResult, String>>>>,
    window: Retained<NSWindow>,
    raw_nonce: String,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppleAuthIvars]
    #[name = "ChinottoAppleAuthDelegate"]
    struct AppleAuthDelegate;

    unsafe impl NSObjectProtocol for AppleAuthDelegate {}

    unsafe impl ASAuthorizationControllerDelegate for AppleAuthDelegate {
        #[unsafe(method(authorizationController:didCompleteWithAuthorization:))]
        unsafe fn authorization_controller_did_complete_with_authorization(
            &self,
            _controller: &ASAuthorizationController,
            authorization: &ASAuthorization,
        ) {
            let result = (|| {
                let cred = authorization.credential();
                let apple = cred
                    .downcast::<ASAuthorizationAppleIDCredential>()
                    .map_err(|_| "unexpected credential type from Sign in with Apple".to_string())?;
                let token_data = apple
                    .identityToken()
                    .ok_or_else(|| "Sign in with Apple did not return an identity token".to_string())?;
                let bytes = unsafe { token_data.as_bytes_unchecked() };
                let id_token = std::str::from_utf8(bytes)
                    .map_err(|_| "identity token was not valid UTF-8".to_string())?
                    .to_string();
                let raw_nonce = self.ivars().raw_nonce.clone();
                Ok(NativeAppleSignInResult { id_token, raw_nonce })
            })();
            send_result(self.ivars(), result);
        }

        #[unsafe(method(authorizationController:didCompleteWithError:))]
        unsafe fn authorization_controller_did_complete_with_error(
            &self,
            _controller: &ASAuthorizationController,
            error: &NSError,
        ) {
            let msg = if error.code() == ASAuthorizationError::Canceled.0 {
                APPLE_SIGN_IN_CANCELLED.to_string()
            } else {
                error.localizedDescription().to_string()
            };
            send_result(self.ivars(), Err(msg));
        }
    }

    unsafe impl ASAuthorizationControllerPresentationContextProviding for AppleAuthDelegate {
        #[unsafe(method_id(presentationAnchorForAuthorizationController:))]
        unsafe fn presentation_anchor_for_authorization_controller(
            &self,
            _controller: &ASAuthorizationController,
        ) -> Retained<ASPresentationAnchor> {
            Retained::cast_unchecked::<ASPresentationAnchor>(self.ivars().window.clone())
        }
    }
);

impl AppleAuthDelegate {
    fn new(
        mtm: MainThreadMarker,
        window: Retained<NSWindow>,
        raw_nonce: String,
        done_tx: SyncSender<Result<NativeAppleSignInResult, String>>,
    ) -> Retained<Self> {
        let ivars = AppleAuthIvars {
            done_tx: Mutex::new(Some(done_tx)),
            window,
            raw_nonce,
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }
}

pub fn run(app: &AppHandle) -> Result<NativeAppleSignInResult, String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let tx_fail = tx.clone();
    let inner = app.clone();
    app.clone().run_on_main_thread(move || {
        let out = run_on_main(&inner, tx);
        if let Err(e) = out {
            let _ = tx_fail.send(Err(e));
        }
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(120))
        .map_err(|_| "Sign in with Apple timed out.".to_string())?
}

/// End an authorization request that no longer has a visible system sheet.
///
/// AuthenticationServices normally reports a cancelled sheet through the delegate. This explicit
/// escape hatch also releases the retained controller and wakes the blocking command when macOS
/// dismisses the sheet without delivering that callback promptly.
pub fn cancel(app: &AppHandle) -> Result<(), String> {
    app.clone()
        .run_on_main_thread(|| {
            let flow = SIGN_IN_KEEP_ALIVE.with(|cell| cell.borrow_mut().take());
            if let Some(flow) = flow {
                if let Some(sender) = flow
                    .delegate
                    .ivars()
                    .done_tx
                    .lock()
                    .expect("AppleAuthIvars.done_tx poisoned")
                    .take()
                {
                    let _ = sender.send(Err(APPLE_SIGN_IN_CANCELLED.to_string()));
                }
                unsafe {
                    flow.controller.cancel();
                }
            }
        })
        .map_err(|e| e.to_string())
}

fn run_on_main(app: &AppHandle, done_tx: SyncSender<Result<NativeAppleSignInResult, String>>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("Sign in with Apple must run on the main thread.")?;

    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let raw: *mut NSWindow = webview
        .ns_window()
        .map_err(|e: tauri::Error| e.to_string())? as *mut NSWindow;
    if raw.is_null() {
        return Err("native window handle was null".to_string());
    }
    let window = unsafe { Retained::retain(raw).ok_or_else(|| "failed to retain NSWindow".to_string())? };

    let raw_nonce = uuid::Uuid::new_v4().to_string();
    let digest = Sha256::digest(raw_nonce.as_bytes());
    let nonce_hex: String = format!("{digest:x}");
    let nonce_ns = NSString::from_str(&nonce_hex);

    let provider = unsafe { ASAuthorizationAppleIDProvider::new() };
    let request = unsafe { provider.createRequest() };
    unsafe {
        request.setNonce(Some(&nonce_ns));
        let scopes = NSArray::from_slice(&[
            ASAuthorizationScopeFullName,
            ASAuthorizationScopeEmail,
        ]);
        request.setRequestedScopes(Some(&scopes));
        request.setRequestedOperation(&ASAuthorizationOperationLogin);
    }

    let auth_req: Retained<ASAuthorizationRequest> = {
        let open = request.into_super();
        open.into_super()
    };
    let requests = NSArray::from_retained_slice(&[auth_req]);

    let controller = unsafe {
        ASAuthorizationController::initWithAuthorizationRequests(
            ASAuthorizationController::alloc(),
            &requests,
        )
    };

    let delegate = AppleAuthDelegate::new(mtm, window, raw_nonce, done_tx);

    unsafe {
        controller.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        controller.setPresentationContextProvider(Some(ProtocolObject::from_ref(&*delegate)));
    }

    SIGN_IN_KEEP_ALIVE.with(|cell| -> Result<(), String> {
        let mut slot = cell.borrow_mut();
        if slot.is_some() {
            return Err("Another Sign in with Apple flow is already in progress.".to_string());
        }
        *slot = Some(SignInKeepAlive {
            delegate,
            controller: controller.clone(),
        });
        Ok(())
    })?;

    unsafe {
        controller.performRequests();
    }
    Ok(())
}
