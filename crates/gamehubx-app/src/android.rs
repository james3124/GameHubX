use gamehubx_platform::set_android_files_dir;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_gamehubx_GameHubXActivity_nativeSetFilesDir<'caller>(
    mut env: jni::EnvUnowned<'caller>,
    _class: jni::objects::JClass<'caller>,
    path: jni::objects::JString<'caller>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let value: String = env.get_string(&path)?.into();
        set_android_files_dir(value);
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}
