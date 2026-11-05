//! JNI entry of the Android launcher's game-file import (th08-repack `import::bridge`).

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_th08_port_GameImport_call<'c>(
    mut env: jni::EnvUnowned<'c>,
    _class: jni::objects::JClass<'c>,
    req: jni::objects::JString<'c>,
) -> jni::objects::JString<'c> {
    env.with_env(|env| -> jni::errors::Result<jni::objects::JString<'c>> {
        let r = req.try_to_string(env)?;
        jni::objects::JString::from_str(env, th08_repack::import::bridge::handle(&r))
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
