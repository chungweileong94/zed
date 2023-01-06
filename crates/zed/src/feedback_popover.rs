use std::{ops::Range, sync::Arc};

use anyhow::bail;
use client::{Client, ZED_SECRET_CLIENT_TOKEN};
use editor::Editor;
use futures::AsyncReadExt;
use gpui::{
    actions,
    elements::{
        AnchorCorner, ChildView, Flex, MouseEventHandler, Overlay, OverlayFitMode, ParentElement,
        Stack, Text,
    },
    serde_json, CursorStyle, Element, ElementBox, Entity, MouseButton, MutableAppContext,
    RenderContext, View, ViewContext, ViewHandle,
};
use isahc::Request;
use lazy_static::lazy_static;
use serde::Serialize;
use settings::Settings;
use workspace::{item::ItemHandle, StatusItemView};

use crate::{feedback_popover, system_specs::SystemSpecs};

lazy_static! {
    pub static ref ZED_SERVER_URL: String =
        std::env::var("ZED_SERVER_URL").unwrap_or_else(|_| "https://zed.dev".to_string());
}

const FEEDBACK_CHAR_COUNT_RANGE: Range<usize> = Range {
    start: 5,
    end: 1000,
};

actions!(feedback, [ToggleFeedbackPopover, SubmitFeedback]);

pub fn init(cx: &mut MutableAppContext) {
    cx.add_action(FeedbackButton::toggle_feedback);
    cx.add_action(FeedbackPopover::submit_feedback);
}

pub struct FeedbackButton {
    feedback_popover: Option<ViewHandle<FeedbackPopover>>,
}

impl FeedbackButton {
    pub fn new() -> Self {
        Self {
            feedback_popover: None,
        }
    }

    pub fn toggle_feedback(&mut self, _: &ToggleFeedbackPopover, cx: &mut ViewContext<Self>) {
        match self.feedback_popover.take() {
            Some(_) => {}
            None => {
                let popover_view = cx.add_view(|_cx| FeedbackPopover::new(_cx));
                self.feedback_popover = Some(popover_view.clone());
            }
        }

        cx.notify();
    }
}

impl Entity for FeedbackButton {
    type Event = ();
}

impl View for FeedbackButton {
    fn ui_name() -> &'static str {
        "FeedbackButton"
    }

    fn render(&mut self, cx: &mut RenderContext<'_, Self>) -> ElementBox {
        Stack::new()
            .with_child(
                MouseEventHandler::<Self>::new(0, cx, |state, cx| {
                    let theme = &cx.global::<Settings>().theme;
                    let theme = &theme.workspace.status_bar.feedback;

                    Text::new(
                        "Give Feedback".to_string(),
                        theme
                            .style_for(state, self.feedback_popover.is_some())
                            .clone(),
                    )
                    .boxed()
                })
                .with_cursor_style(CursorStyle::PointingHand)
                .on_click(MouseButton::Left, |_, cx| {
                    cx.dispatch_action(ToggleFeedbackPopover)
                })
                .boxed(),
            )
            .with_children(self.feedback_popover.as_ref().map(|popover| {
                Overlay::new(ChildView::new(popover, cx).contained().boxed())
                    .with_fit_mode(OverlayFitMode::SwitchAnchor)
                    .with_anchor_corner(AnchorCorner::TopLeft)
                    .with_z_index(999)
                    .boxed()
            }))
            .boxed()
    }
}

impl StatusItemView for FeedbackButton {
    fn set_active_pane_item(
        &mut self,
        _: Option<&dyn ItemHandle>,
        _: &mut gpui::ViewContext<Self>,
    ) {
        // N/A
    }
}

pub struct FeedbackPopover {
    feedback_editor: ViewHandle<Editor>,
    // _subscriptions: Vec<Subscription>,
}

impl Entity for FeedbackPopover {
    type Event = ();
}

#[derive(Serialize)]
struct FeedbackRequestBody<'a> {
    feedback_text: &'a str,
    metrics_id: Option<Arc<str>>,
    system_specs: SystemSpecs,
    token: &'a str,
}

impl FeedbackPopover {
    pub fn new(cx: &mut ViewContext<Self>) -> Self {
        let feedback_editor = cx.add_view(|cx| {
            let editor = Editor::multi_line(
                Some(Arc::new(|theme| theme.feedback.feedback_editor.clone())),
                cx,
            );
            editor
        });

        cx.focus(&feedback_editor);

        cx.subscribe(&feedback_editor, |this, _, event, cx| {
            if let editor::Event::BufferEdited = event {
                let buffer_len = this.feedback_editor.read(cx).buffer().read(cx).len(cx);
                let feedback_chars_remaining = FEEDBACK_CHAR_COUNT_RANGE.end - buffer_len;
                dbg!(feedback_chars_remaining);
            }
        })
        .detach();

        // let active_call = ActiveCall::global(cx);
        // let mut subscriptions = Vec::new();
        // subscriptions.push(cx.observe(&user_store, |this, _, cx| this.update_entries(cx)));
        // subscriptions.push(cx.observe(&active_call, |this, _, cx| this.update_entries(cx)));
        let this = Self {
            feedback_editor, // _subscriptions: subscriptions,
        };
        // this.update_entries(cx);
        this
    }

    fn submit_feedback(&mut self, _: &SubmitFeedback, cx: &mut ViewContext<'_, Self>) {
        let feedback_text = self.feedback_editor.read(cx).text(cx);
        let zed_client = cx.global::<Arc<Client>>();
        let system_specs = SystemSpecs::new(cx);
        let feedback_endpoint = format!("{}/api/feedback", *ZED_SERVER_URL);

        let metrics_id = zed_client.metrics_id();
        let http_client = zed_client.http_client();

        cx.spawn(|_, _| {
            async move {
                // TODO FEEDBACK: Use or remove
                // this.read_with(&async_cx, |this, cx| {
                //     // Now we have a &self and a &AppContext
                // });

                let request = FeedbackRequestBody {
                    feedback_text: &feedback_text,
                    metrics_id,
                    system_specs,
                    token: ZED_SECRET_CLIENT_TOKEN,
                };

                let json_bytes = serde_json::to_vec(&request)?;

                let request = Request::post(feedback_endpoint)
                    .header("content-type", "application/json")
                    .body(json_bytes.into())?;

                let mut response = http_client.send(request).await?;
                let mut body = String::new();
                response.body_mut().read_to_string(&mut body).await?;

                let response_status = response.status();

                dbg!(response_status);

                if !response_status.is_success() {
                    // TODO FEEDBACK: Do some sort of error reporting here for if store fails
                    bail!("Error")
                }

                // TODO FEEDBACK: Use or remove
                // Will need to handle error cases
                // async_cx.update(|cx| {
                //     this.update(cx, |this, cx| {
                //         this.handle_error(error);
                //         cx.notify();
                //         cx.dispatch_action(ShowErrorPopover);
                //         this.error_text = "Embedding failed"
                //     })
                // });

                Ok(())
            }
        })
        .detach();
    }
}

impl View for FeedbackPopover {
    fn ui_name() -> &'static str {
        "FeedbackPopover"
    }

    fn render(&mut self, cx: &mut RenderContext<Self>) -> ElementBox {
        enum SubmitFeedback {}

        let theme = cx.global::<Settings>().theme.clone();
        let submit_feedback_text_button_height = 20.0;

        Flex::column()
            .with_child(
                Flex::row()
                    .with_child(
                        ChildView::new(self.feedback_editor.clone(), cx)
                            .contained()
                            .with_style(theme.feedback.feedback_editor.container)
                            .flex(1., true)
                            .boxed(),
                    )
                    .constrained()
                    .with_width(theme.feedback.feedback_popover.width)
                    .with_height(
                        theme.feedback.feedback_popover.height - submit_feedback_text_button_height,
                    )
                    .boxed(),
            )
            .with_child(
                MouseEventHandler::<SubmitFeedback>::new(0, cx, |state, _| {
                    let theme = &theme.workspace.status_bar.feedback;

                    Text::new(
                        "Submit Feedback".to_string(),
                        theme.style_for(state, true).clone(),
                    )
                    .constrained()
                    .with_height(submit_feedback_text_button_height)
                    .boxed()
                })
                .with_cursor_style(CursorStyle::PointingHand)
                .on_click(MouseButton::Left, |_, cx| {
                    cx.dispatch_action(feedback_popover::SubmitFeedback)
                })
                .on_click(MouseButton::Left, |_, cx| {
                    cx.dispatch_action(feedback_popover::ToggleFeedbackPopover)
                })
                .boxed(),
            )
            .contained()
            .with_style(theme.feedback.feedback_popover.container)
            .constrained()
            .with_width(theme.feedback.feedback_popover.width)
            .with_height(theme.feedback.feedback_popover.height)
            .boxed()
    }
}
