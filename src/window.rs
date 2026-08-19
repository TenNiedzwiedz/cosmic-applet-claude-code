// SPDX-License-Identifier: MIT

use chrono::{Datelike, Weekday};
use cosmic::applet::{cosmic_panel_config::PanelAnchor, padded_control};
use cosmic::cosmic_theme::Spacing;
use cosmic::iced::{
    Alignment, Length, Subscription,
    platform_specific::shell::wayland::commands::popup::destroy_popup,
    time,
    widget::{column, row},
    window::Id,
};
use cosmic::widget::{autosize, button, container, divider, icon, progress_bar, text};
use cosmic::{Element, Task, app, theme};
use std::sync::LazyLock;
use std::time::Duration;

use crate::data;
use crate::data::model::{AppData, RateWindow, Session};
use crate::fl;

pub const APP_ID: &str = "io.github.tenniedzwiedz.CosmicAppletClaudeCode";

const ICON_SVG: &[u8] =
    include_bytes!("../data/icons/io.github.tenniedzwiedz.CosmicAppletClaudeCode-symbolic.svg");

/// Both source directories hold a handful of small JSON files, so re-reading
/// them beats maintaining inotify watches across the atomic replaces that
/// produce them.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

const BUSY_DOT: &str = "\u{25cf}";
const SEPARATOR: &str = " \u{b7} ";

static AUTOSIZE_MAIN_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("autosize-main"));

pub struct Window {
    core: app::Core,
    popup: Option<Id>,
    data: AppData,
    now: i64,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Tick,
}

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn init(core: app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let window = Self {
            core,
            popup: None,
            data: data::collect(),
            now: data::snapshots::now(),
        };

        (window, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        time::every(POLL_INTERVAL).map(|_| Message::Tick)
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::Tick => self.refresh(),
            Message::TogglePopup => {
                return if let Some(popup) = self.popup.take() {
                    destroy_popup(popup)
                } else {
                    self.refresh();

                    cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                        |_| Default::default(),
                        |app: &mut Self| {
                            let new_id = Id::unique();
                            app.popup = Some(new_id);
                            app.core.applet.get_popup_settings(
                                app.core.main_window_id().unwrap(),
                                new_id,
                                Some((1, 1)),
                                None,
                                None,
                            )
                        },
                        None,
                    ))
                };
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );

        let applet_icon = icon::icon(icon::from_svg_bytes(ICON_SVG).symbolic(true))
            .size(self.core.applet.suggested_size(true).0);

        let content: Element<'_, Self::Message> = if horizontal {
            // The dot is a separate widget so it alone can carry the accent
            // colour while a session is working.
            let dot = self
                .core
                .applet
                .text(BUSY_DOT)
                .class(if self.data.any_busy() {
                    theme::Text::Accent
                } else {
                    theme::Text::Default
                });

            let mut parts = row![
                applet_icon,
                dot,
                self.core.applet.text(self.data.sessions.len().to_string())
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            if let Some(percent) = self.data.five_hour_percent(self.now) {
                parts = parts
                    // Extra padding on both sides of the separator, on top of
                    // the row spacing.
                    .push(container(self.core.applet.text(SEPARATOR)).padding([0, 4]))
                    .push(self.core.applet.text(format!("{percent:.0}%")));
            }

            parts.into()
        } else {
            // A vertical panel has no room for the percentage.
            column![
                applet_icon,
                self.core.applet.text(self.data.sessions.len().to_string())
            ]
            .spacing(2)
            .align_x(Alignment::Center)
            .into()
        };

        let padding = self.core.applet.suggested_padding(true).0;
        let button = button::custom(content)
            .padding(if horizontal {
                [0, padding]
            } else {
                [padding, 0]
            })
            .on_press_down(Message::TogglePopup)
            .class(theme::Button::AppletIcon);

        autosize::autosize(button, AUTOSIZE_MAIN_ID.clone()).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let Spacing {
            space_xxxs,
            space_xxs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        let mut content = column![].padding([space_xxs, 0]).spacing(space_xxxs);

        if self.data.sessions.is_empty() {
            content = content.push(padded_control(text::body(fl!("no-sessions"))));
        } else {
            for session in &self.data.sessions {
                content = content.push(padded_control(session_row(session)));
            }
        }

        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));

        match self
            .data
            .limits
            .as_ref()
            .filter(|limits| !limits.is_expired(self.now))
        {
            Some(limits) => {
                if let Some(window) = limits.five_hour {
                    content =
                        content.push(padded_control(self.limit_row(fl!("five-hour"), window)));
                }
                if let Some(window) = limits.seven_day {
                    content =
                        content.push(padded_control(self.limit_row(fl!("seven-day"), window)));
                }

                let age = limits.age_seconds(self.now);
                if age > 60 {
                    content = content.push(padded_control(text::caption(fl!(
                        "captured-ago",
                        time = format_duration(age)
                    ))));
                }
            }
            None => {
                content = content.push(padded_control(text::body(fl!("no-limits"))));
                content = content.push(padded_control(text::caption(fl!("no-limits-hint"))));
            }
        }

        self.core.applet.popup_container(content).into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl Window {
    fn refresh(&mut self) {
        self.data = data::collect();
        self.now = data::snapshots::now();
    }

    fn limit_row(&self, label: String, window: RateWindow) -> Element<'_, Message> {
        let percent = window.used_percentage;
        let reset = match window.seconds_until_reset(self.now) {
            Some(seconds) if seconds < 24 * 3600 => {
                fl!("resets-in", time = format_duration(seconds))
            }
            Some(_) => fl!("resets-at", time = format_reset_date(window.resets_at)),
            None => String::new(),
        };

        let heading = row![
            text::body(label).width(Length::Fill),
            text::body(format!("{percent:.0}%")),
        ]
        .align_y(Alignment::Center);

        let bar = progress_bar::determinate_linear((percent / 100.0) as f32)
            .width(Length::Fill)
            .girth(Length::Fixed(4.0));

        let mut rows = column![heading, bar].spacing(4);
        if !reset.is_empty() {
            rows = rows.push(text::caption(reset));
        }

        container(rows).width(Length::Fill).into()
    }
}

fn session_row(session: &Session) -> Element<'_, Message> {
    let status = text::caption(if session.is_busy() {
        fl!("status-busy")
    } else {
        fl!("status-idle")
    })
    .class(if session.is_busy() {
        theme::Text::Accent
    } else {
        theme::Text::Default
    });

    let mut details = vec![session.dir_label().to_string()];
    if let Some(percent) = session.context_percent {
        details.push(fl!("context", percent = format!("{percent:.0}")));
    }
    if let Some(model) = session.model.as_deref() {
        details.push(model.to_string());
    }

    column![
        row![text::body(session.name.clone()).width(Length::Fill), status]
            .align_y(Alignment::Center),
        text::caption(details.join(SEPARATOR)),
    ]
    .width(Length::Fill)
    .into()
}

/// `● 3 · 50%` - the dot is drawn in the accent colour while a session is
/// working. Shared with `--dump` so the debug output matches the panel.
pub fn panel_label(data: &AppData, now: i64) -> String {
    let count = data.sessions.len();

    match data.five_hour_percent(now) {
        Some(percent) => format!("{BUSY_DOT} {count}{SEPARATOR}{percent:.0}%"),
        None => format!("{BUSY_DOT} {count}"),
    }
}

/// Deliberately compact: the popup has little room and the numbers matter more
/// than the wording.
fn format_duration(seconds: i64) -> String {
    let minutes = seconds / 60;

    if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else if minutes >= 1 {
        format!("{minutes}m")
    } else {
        "<1m".to_string()
    }
}

fn format_reset_date(timestamp: i64) -> String {
    let Some(local) =
        chrono::DateTime::from_timestamp(timestamp, 0).map(|utc| utc.with_timezone(&chrono::Local))
    else {
        return String::new();
    };

    // chrono only formats weekday names in English, and the popup is localised.
    let weekday = match local.weekday() {
        Weekday::Mon => fl!("weekday-mon"),
        Weekday::Tue => fl!("weekday-tue"),
        Weekday::Wed => fl!("weekday-wed"),
        Weekday::Thu => fl!("weekday-thu"),
        Weekday::Fri => fl!("weekday-fri"),
        Weekday::Sat => fl!("weekday-sat"),
        Weekday::Sun => fl!("weekday-sun"),
    };

    format!("{weekday} {}", local.format("%H:%M"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::model::{Limits, SessionStatus};

    fn session(name: &str, status: SessionStatus) -> Session {
        Session {
            pid: 1,
            session_id: name.to_string(),
            name: name.to_string(),
            cwd: format!("/home/u/{name}"),
            status,
            started_at: None,
            version: None,
            context_percent: None,
            model: None,
            cost_usd: None,
        }
    }

    #[test]
    fn durations_are_compact() {
        assert_eq!(format_duration(30), "<1m");
        assert_eq!(format_duration(90), "1m");
        assert_eq!(format_duration(4320), "1h 12m");
        assert_eq!(format_duration(36_000), "10h 00m");
    }

    #[test]
    fn panel_label_drops_the_percentage_once_the_window_resets() {
        let mut data = AppData {
            sessions: vec![
                session("a", SessionStatus::Idle),
                session("b", SessionStatus::Busy),
            ],
            limits: Some(Limits {
                five_hour: Some(RateWindow {
                    used_percentage: 67.6,
                    resets_at: 2_000,
                }),
                seven_day: None,
                captured_at: 1_000,
                source_session: "b".into(),
            }),
        };

        // Busy state is carried by the dot's colour, not by the text.
        assert_eq!(panel_label(&data, 1_500), "\u{25cf} 2 \u{b7} 68%");
        assert!(data.any_busy());

        assert_eq!(panel_label(&data, 3_000), "\u{25cf} 2");

        data.limits = None;
        assert_eq!(panel_label(&data, 1_500), "\u{25cf} 2");
        data.sessions[1].status = SessionStatus::Idle;
        assert!(!data.any_busy());
    }
}
