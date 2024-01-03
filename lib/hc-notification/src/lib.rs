use std::time::Duration;

use hc_foundation::{LocalExecutor, Result, Timer};

#[cfg(target_os = "windows")]
pub fn notify(executor: &LocalExecutor<'_>, msg_title: &str, msg_body: &str, duration: Duration) -> Result<()> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastTemplateType};

    let toast_xml = ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02)?;
    let toast_text_elements = toast_xml.GetElementsByTagName(&HSTRING::from("text"))?;

    toast_text_elements
        .GetAt(0)?
        .AppendChild(&toast_xml.CreateTextNode(&HSTRING::from(msg_title))?)?;

    toast_text_elements
        .GetAt(1)?
        .AppendChild(&toast_xml.CreateTextNode(&HSTRING::from(msg_body))?)?;

    let toast = ToastNotification::CreateToastNotification(&toast_xml)?;

    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("HeadsetController"))?;
    notifier.Show(&toast)?;
    executor
        .spawn(async move {
            Timer::after(duration).await;
            notifier
                .Hide(&toast)
                .unwrap_or_else(|err| tracing::warn!("Can not hide notification: {}", err));
        })
        .detach();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn notify(executor: &LocalExecutor<'_>, msg_title: &str, msg_body: &str, duration: Duration) -> Result<()> {
    notify_rust::Notification::new()
        .summary(msg_title)
        .body(msg_body)
        .timeout(duration)
        .show()?;
    Ok(())
}

