//! Turns readings into the requests the deck draws, and decides when to send them.
//!
//! A tile or page is sent when what it would show differs from what it last showed, never
//! more than once per [`SEND_FLOOR`]. A change inside the floor waits for it, and a burst
//! of changes sends only the last.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use td_core::layout::{Action, Page};
use td_core::{DashboardRow, Layout, Request};

use super::{CLIPROXYAPI, Reading, SEND_FLOOR};

const VALUE_MAX: usize = 24;
const CAPTION_MAX: usize = 16;
const ROW_MAX: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WidgetBinding {
    tile: String,
    caption: String,
    source: String,
    account: String,
    field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashboardBinding {
    page: String,
    source: String,
}

struct Sent {
    request: Request,
    at: Instant,
}

fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[derive(Default)]
pub struct Scheduler {
    widgets: Vec<WidgetBinding>,
    dashboards: Vec<DashboardBinding>,
    readings: HashMap<String, Vec<Reading>>,
    sent: HashMap<String, Sent>,
}

impl Scheduler {
    pub fn from_layout(layout: &Layout) -> Scheduler {
        let mut scheduler = Scheduler::default();
        scheduler.set_layout(Some(layout));
        scheduler
    }

    /// Binds the layout's widget tiles and dashboard pages. Readings stay, and every tile
    /// is sent again.
    pub fn set_layout(&mut self, layout: Option<&Layout>) {
        self.widgets.clear();
        self.dashboards.clear();
        self.sent.clear();
        let Some(layout) = layout else {
            return;
        };
        for page in &layout.pages {
            match page {
                Page::Grid { tiles, .. } => {
                    for tile in tiles {
                        if let Action::Widget {
                            source,
                            field,
                            account,
                        } = &tile.action
                        {
                            self.widgets.push(WidgetBinding {
                                tile: tile.id.clone(),
                                caption: cut(&tile.label, CAPTION_MAX),
                                source: source.clone(),
                                account: account.clone(),
                                field: field.clone(),
                            });
                        }
                    }
                }
                Page::Dashboard { id, source, .. } => self.dashboards.push(DashboardBinding {
                    page: id.clone(),
                    source: source.clone(),
                }),
            }
        }
    }

    /// The widget tiles and the dashboard pages bound.
    pub fn counts(&self) -> (usize, usize) {
        (self.widgets.len(), self.dashboards.len())
    }

    /// The source a tile shows, for a tap that should refresh it.
    pub fn source_of_tile(&self, tile: &str) -> Option<&str> {
        self.widgets
            .iter()
            .find(|binding| binding.tile == tile)
            .map(|binding| binding.source.as_str())
    }

    pub fn update(&mut self, source: &str, readings: Vec<Reading>) {
        self.readings.insert(source.to_owned(), readings);
    }

    /// Forgets what was sent, as after a new connection, so everything is sent again.
    pub fn resync(&mut self) {
        self.sent.clear();
    }

    /// Forgets one send that failed, so it comes due again.
    pub fn forget(&mut self, request: &Request) {
        self.sent.retain(|_, sent| &sent.request != request);
    }

    fn widget(&self, binding: &WidgetBinding) -> Option<Request> {
        let readings = self.readings.get(&binding.source)?;
        let reading = readings
            .iter()
            .find(|reading| reading.field == binding.field && reading.account == binding.account);
        // A failed provider or a missing window must clear the previous quota on deck.
        let Some(reading) = reading else {
            if binding.source == CLIPROXYAPI
                && matches!(
                    binding.field.as_str(),
                    "five_hour_pct"
                        | "seven_day_pct"
                        | "five_hour_reset"
                        | "seven_day_reset"
                        | "plan"
                )
            {
                let status = readings
                    .iter()
                    .find(|r| r.field == "status" && r.account == binding.account)
                    .or_else(|| {
                        readings
                            .iter()
                            .find(|r| r.field == "status" && r.account.is_empty())
                    })?;
                return Some(Request::WidgetSet {
                    tile: binding.tile.clone(),
                    value: "unavailable".to_owned(),
                    caption: binding.caption.clone(),
                    bar: -1.0,
                    stale: status.stale,
                });
            }
            return None;
        };
        Some(Request::WidgetSet {
            tile: binding.tile.clone(),
            value: cut(&reading.value, VALUE_MAX),
            caption: binding.caption.clone(),
            bar: if reading.bar < 0.0 {
                -1.0
            } else {
                reading.bar.min(1.0)
            },
            stale: reading.stale,
        })
    }

    fn dashboard(&self, binding: &DashboardBinding) -> Option<Request> {
        if binding.source != CLIPROXYAPI {
            return None;
        }
        let readings = self.readings.get(&binding.source)?;
        if !readings.iter().any(|reading| reading.field == "status") {
            return None;
        }
        let rows = readings
            .iter()
            .filter(|reading| reading.field == "status" && !reading.account.is_empty())
            .take(ROW_MAX)
            .map(|status| {
                let field = |name| {
                    readings
                        .iter()
                        .find(|r| r.field == name && r.account == status.account)
                };
                let five = field("five_hour_pct");
                let seven = field("seven_day_pct");
                let usage = five.is_some() || seven.is_some();
                let bar = if usage {
                    five.into_iter()
                        .chain(seven)
                        .map(|r| r.bar)
                        .fold(0.0, f64::max)
                } else {
                    field("recent_10m").map_or(-1.0, |r| r.bar)
                };
                let (provider, email) = status
                    .account
                    .split_once(':')
                    .unwrap_or(("", &status.account));
                let local = email.split('@').next().unwrap_or(email);
                DashboardRow {
                    label: cut(format!("{provider} {local}").trim(), CAPTION_MAX),
                    value: if status.stale {
                        "stale".to_owned()
                    } else if usage {
                        format!(
                            "5h {} 7d {}",
                            five.map_or("—", |r| r.value.as_str()),
                            seven.map_or("—", |r| r.value.as_str())
                        )
                    } else {
                        cut(&status.value, CAPTION_MAX)
                    },
                    bar: if status.stale { -1.0 } else { bar },
                }
            })
            .collect();
        Some(Request::DashboardSet {
            page: binding.page.clone(),
            rows,
        })
    }

    fn wanted(&self) -> Vec<(String, Request)> {
        let widgets = self
            .widgets
            .iter()
            .filter_map(|b| Some((format!("widget:{}", b.tile), self.widget(b)?)));
        let dashboards = self
            .dashboards
            .iter()
            .filter_map(|b| Some((format!("dashboard:{}", b.page), self.dashboard(b)?)));
        widgets.chain(dashboards).collect()
    }

    /// The requests to send now.
    pub fn due(&mut self, now: Instant) -> Vec<Request> {
        let mut out = Vec::new();
        for (key, request) in self.wanted() {
            let waiting = match self.sent.get(&key) {
                Some(sent) if sent.request == request => continue,
                Some(sent) => now.saturating_duration_since(sent.at) < SEND_FLOOR,
                None => false,
            };
            if !waiting {
                self.sent.insert(
                    key,
                    Sent {
                        request: request.clone(),
                        at: now,
                    },
                );
                out.push(request);
            }
        }
        out
    }

    /// How long until a request held back by the floor comes due, if one is held back.
    pub fn pending_in(&self, now: Instant) -> Option<Duration> {
        self.wanted()
            .into_iter()
            .filter_map(|(key, request)| {
                let sent = self.sent.get(&key).filter(|sent| sent.request != request)?;
                SEND_FLOOR.checked_sub(now.saturating_duration_since(sent.at))
            })
            .min()
    }
}
