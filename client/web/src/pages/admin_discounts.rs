use crate::components::admin_nav::AdminNav;
use crate::net::{mutation, query};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

/// Extract a human-readable message from a tRPC error string.
///
/// `parse_batch_single` returns the full error JSON object as a string when the
/// server responds with `[{"error":{...}}]`. Try to pull `error.message`; fall
/// back to the raw string for plain network/parse errors.
fn trpc_err_msg(e: String) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&e) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    e
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Discount {
    id: String,
    code: String,
    name: String,
    amount_type: String,
    amount: i64,
    duration: String,
    duration_in_months: Option<i64>,
    max_redemptions: Option<i64>,
    times_redeemed: i64,
    expires_at: Option<String>,
    is_active: bool,
    test_mode: bool,
}

fn format_amount(d: &Discount) -> String {
    if d.amount_type == "PERCENT" {
        format!("{}%", d.amount)
    } else {
        // stored as cents
        format!("${:.2}", d.amount as f64 / 100.0)
    }
}

fn format_duration(d: &Discount) -> &'static str {
    match d.duration.as_str() {
        "ONCE" => "Once",
        "FOREVER" => "Forever",
        _ => "Repeating",
    }
}

fn format_expiry(s: &Option<String>) -> String {
    match s {
        None => "—".to_string(),
        Some(v) => {
            // ISO date string: take the date part only (before 'T')
            v.split('T').next().unwrap_or(v).to_string()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Create,
    Discounts,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Create => "Create",
            Panel::Discounts => "Discounts",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Create, 16.0, 16.0, 560.0, 880.0),
        b.at(Panel::Discounts, 592.0, 16.0, 1312.0, 880.0),
    ]
}

#[component]
pub fn AdminDiscounts() -> Element {
    let mut discounts = use_signal(Vec::<Discount>::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    // track which ids are being mutated for disable state
    let mut saving_ids = use_signal(Vec::<String>::new);
    let mut message = use_signal(String::new);
    let mut error_msg = use_signal(String::new);

    // form fields
    let mut f_code = use_signal(String::new);
    let mut f_name = use_signal(String::new);
    let mut f_amount_type = use_signal(|| "PERCENT".to_string());
    // raw string so empty = not provided
    let mut f_amount = use_signal(String::new);
    let mut f_duration = use_signal(|| "ONCE".to_string());
    let mut f_duration_months = use_signal(String::new);
    let mut f_max_redemptions = use_signal(String::new);
    let mut f_expires_at = use_signal(String::new); // YYYY-MM-DD from date input
    let mut f_test_mode = use_signal(|| false);

    let refresh = move || {
        spawn_local(async move {
            match query("discount.listForAdmin", None).await {
                Ok(v) => {
                    let parsed: Vec<Discount> = serde_json::from_value(v).unwrap_or_default();
                    discounts.set(parsed);
                }
                Err(e) => error_msg.set(trpc_err_msg(e)),
            }
        });
    };

    use_effect(move || {
        loading.set(true);
        spawn_local(async move {
            match query("discount.listForAdmin", None).await {
                Ok(v) => {
                    let parsed: Vec<Discount> = serde_json::from_value(v).unwrap_or_default();
                    discounts.set(parsed);
                }
                Err(e) => error_msg.set(trpc_err_msg(e)),
            }
            loading.set(false);
        });
    });

    let create_code = move |_: FormEvent| {
        let code = f_code.read().trim().to_uppercase();
        let name = f_name.read().trim().to_string();
        let amount_type = f_amount_type.read().clone();
        let duration = f_duration.read().clone();

        let raw_amount: f64 = match f_amount.read().parse() {
            Ok(v) => v,
            Err(_) => return,
        };
        if !raw_amount.is_finite() || raw_amount <= 0.0 {
            return;
        }
        // FIXED: front-end enters dollars, server wants cents
        let amount = if amount_type == "FIXED" {
            (raw_amount * 100.0).round() as i64
        } else {
            raw_amount as i64
        };

        let duration_in_months: Option<i64> = if duration == "REPEATING" {
            f_duration_months
                .read()
                .parse::<i64>()
                .ok()
                .filter(|&n| n > 0)
        } else {
            None
        };
        let max_redemptions: Option<i64> = f_max_redemptions
            .read()
            .parse::<i64>()
            .ok()
            .filter(|&n| n > 0);
        // date input gives YYYY-MM-DD; zod wants RFC3339
        let expires_at: Option<String> = {
            let v = f_expires_at.read().trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some(format!("{v}T00:00:00.000Z"))
            }
        };
        let test_mode = *f_test_mode.read();

        saving.set(true);
        message.set(String::new());
        error_msg.set(String::new());

        spawn_local(async move {
            let mut input = json!({
                "code": code,
                "name": name,
                "amountType": amount_type,
                "amount": amount,
                "duration": duration,
                "testMode": test_mode,
            });
            if let Some(dm) = duration_in_months {
                input["durationInMonths"] = json!(dm);
            }
            if let Some(mr) = max_redemptions {
                input["maxRedemptions"] = json!(mr);
            }
            if let Some(ea) = expires_at {
                input["expiresAt"] = json!(ea);
            }

            match mutation("discount.create", Some(input)).await {
                Ok(_) => {
                    message.set(format!("Created code {code}."));
                    // reset form
                    f_code.set(String::new());
                    f_name.set(String::new());
                    f_amount_type.set("PERCENT".to_string());
                    f_amount.set(String::new());
                    f_duration.set("ONCE".to_string());
                    f_duration_months.set(String::new());
                    f_max_redemptions.set(String::new());
                    f_expires_at.set(String::new());
                    f_test_mode.set(false);
                    refresh();
                }
                Err(e) => error_msg.set(trpc_err_msg(e)),
            }
            saving.set(false);
        });
    };

    let ws = use_workspace("admin_discounts_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Create => rsx! {
                form {
                    style: "display:grid;gap:0.75rem;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));align-items:end;overflow-y:auto;padding:0.5rem",
                    onsubmit: create_code,

                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Code"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;font-family:monospace;text-transform:uppercase",
                            r#type: "text",
                            placeholder: "LAUNCH50",
                            minlength: "3",
                            maxlength: "256",
                            required: true,
                            value: "{f_code}",
                            oninput: move |e| f_code.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Name"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "text",
                            placeholder: "Launch promo",
                            minlength: "2",
                            maxlength: "120",
                            required: true,
                            value: "{f_name}",
                            oninput: move |e| f_name.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Amount type"
                        select {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                            value: "{f_amount_type}",
                            oninput: move |e| f_amount_type.set(e.value()),
                            option { value: "PERCENT", "Percent" }
                            option { value: "FIXED", "Fixed" }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Amount"
                        div { class: "row", style: "gap:0.25rem;align-items:center",
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;flex:1",
                                r#type: "number",
                                min: "1",
                                step: "1",
                                required: true,
                                value: "{f_amount}",
                                oninput: move |e| f_amount.set(e.value()),
                            }
                            span { class: "muted", style: "font-size:0.75rem;white-space:nowrap",
                                if f_amount_type.read().as_str() == "PERCENT" { "%" } else { "USD" }
                            }
                        }
                        if f_amount_type.read().as_str() == "FIXED" {
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none", "Enter dollars (e.g. 10 = $10.00)" }
                        } else {
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none", "1–100" }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Duration"
                        select {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                            value: "{f_duration}",
                            oninput: move |e| f_duration.set(e.value()),
                            option { value: "ONCE", "Once (first payment only)" }
                            option { value: "FOREVER", "Forever" }
                            option { value: "REPEATING", "Repeating" }
                        }
                    }
                    if f_duration.read().as_str() == "REPEATING" {
                        label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                            "Duration in months"
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                                r#type: "number",
                                min: "1",
                                step: "1",
                                placeholder: "e.g. 3",
                                value: "{f_duration_months}",
                                oninput: move |e| f_duration_months.set(e.value()),
                            }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Max redemptions"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "number",
                            min: "1",
                            step: "1",
                            placeholder: "Unlimited",
                            value: "{f_max_redemptions}",
                            oninput: move |e| f_max_redemptions.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Expires at"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "date",
                            value: "{f_expires_at}",
                            oninput: move |e| f_expires_at.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Test mode"
                        div { class: "row", style: "gap:0.5rem;align-items:center;padding:0.5rem 0",
                            input {
                                r#type: "checkbox",
                                style: "width:1rem;height:1rem;cursor:pointer",
                                checked: *f_test_mode.read(),
                                oninput: move |e| f_test_mode.set(e.value() == "true"),
                            }
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none",
                                "Test-mode codes only work on test-mode checkouts."
                            }
                        }
                    }
                    div { style: "display:flex;align-items:flex-end",
                        button {
                            class: "app-btn app-btn-active",
                            style: "height:38px;font-weight:bold",
                            r#type: "submit",
                            disabled: *saving.read(),
                            if *saving.read() { "Saving…" } else { "Create code" }
                        }
                    }
                }
            },
            Panel::Discounts => rsx! {
                div { class: "col", style: "gap:0.75rem;height:100%;overflow:hidden",
                    if !message.read().is_empty() {
                        div { class: "app-card success", style: "padding:0.75rem;font-size:0.875rem",
                            {message.read().clone()}
                        }
                    }
                    if !error_msg.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {error_msg.read().clone()}
                        }
                    }
                    div { style: "overflow-x:auto;flex:1",
                        table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                            thead {
                                tr { style: "font-size:0.75rem;text-transform:uppercase;font-family:monospace",
                                    for col in ["Code", "Name", "Amount", "Duration", "Redemptions", "Expires", "Test", "Status", "Actions"] {
                                        th { class: "muted", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)", {col} }
                                    }
                                }
                            }
                            tbody {
                                for discount in discounts.read().iter() {
                                    {
                                        let did = discount.id.clone();
                                        let dcode = discount.code.clone();
                                        let is_active = discount.is_active;
                                        let amount_str = format_amount(discount);
                                        let duration_str = format_duration(discount);
                                        let expiry_str = format_expiry(&discount.expires_at);
                                        let test_mode = discount.test_mode;
                                        let times = discount.times_redeemed;
                                        let max_red = discount.max_redemptions;
                                        let dname = discount.name.clone();

                                        let did_active = did.clone();
                                        let did_remove = did.clone();
                                        let dcode_msg = dcode.clone();

                                        rsx! {
                                            tr { style: "border-bottom:1px solid var(--border-app)",
                                                td { style: "padding:0.75rem 1rem;font-family:monospace;font-weight:bold",
                                                    {dcode.clone()}
                                                }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {dname} }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {amount_str} }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {duration_str} }
                                                td { class: "muted", style: "padding:0.75rem 1rem",
                                                    {format!("{} / {}", times, max_red.map(|n| n.to_string()).unwrap_or_else(|| "∞".to_string()))}
                                                }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {expiry_str} }
                                                td { style: "padding:0.75rem 1rem",
                                                    if test_mode {
                                                        span { style: "border:1px solid var(--border-app);padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;color:var(--text-secondary)",
                                                            "Test"
                                                        }
                                                    } else {
                                                        span { class: "muted", "—" }
                                                    }
                                                }
                                                td { style: "padding:0.75rem 1rem",
                                                    span {
                                                        style: if is_active {
                                                            "padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--color-success);color:#0f172a"
                                                        } else {
                                                            "padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--border-app);color:var(--text-secondary)"
                                                        },
                                                        if is_active { "Active" } else { "Inactive" }
                                                    }
                                                }
                                                td { style: "padding:0.75rem 1rem",
                                                    {
                                                        let is_busy = saving_ids.read().contains(&did);
                                                        rsx! {
                                                            div { class: "row", style: "gap:0.5rem",
                                                                button {
                                                                    class: "app-btn",
                                                                    style: "font-size:0.75rem;padding:0.25rem 0.5rem",
                                                                    disabled: is_busy,
                                                                    onclick: move |_| {
                                                                        let id = did_active.clone();
                                                                        let next_active = !is_active;
                                                                        let code = dcode_msg.clone();
                                                                        saving_ids.write().push(id.clone());
                                                                        message.set(String::new());
                                                                        error_msg.set(String::new());
                                                                        spawn_local(async move {
                                                                            match mutation("discount.setActive", Some(json!({"id": id, "isActive": next_active}))).await {
                                                                                Ok(_) => {
                                                                                    let state = if next_active { "active" } else { "inactive" };
                                                                                    message.set(format!("{code} is now {state}."));
                                                                                    refresh();
                                                                                }
                                                                                Err(e) => {
                                                                                    error_msg.set(trpc_err_msg(e));
                                                                                    refresh();
                                                                                }
                                                                            }
                                                                            saving_ids.write().retain(|x| x != &id);
                                                                        });
                                                                    },
                                                                    if is_active { "Deactivate" } else { "Activate" }
                                                                }
                                                                button {
                                                                    class: "app-btn error",
                                                                    style: "font-size:0.75rem;padding:0.25rem 0.5rem",
                                                                    disabled: is_busy,
                                                                    onclick: move |_| {
                                                                        let id = did_remove.clone();
                                                                        let code = dcode.clone();
                                                                        saving_ids.write().push(id.clone());
                                                                        message.set(String::new());
                                                                        error_msg.set(String::new());
                                                                        spawn_local(async move {
                                                                            match mutation("discount.remove", Some(json!({"id": id}))).await {
                                                                                Ok(_) => {
                                                                                    message.set(format!("Deleted code {code}."));
                                                                                    refresh();
                                                                                }
                                                                                Err(e) => {
                                                                                    error_msg.set(trpc_err_msg(e));
                                                                                    refresh();
                                                                                }
                                                                            }
                                                                            saving_ids.write().retain(|x| x != &id);
                                                                        });
                                                                    },
                                                                    "Delete"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if discounts.read().is_empty() && !*loading.read() {
                                    tr {
                                        td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "9",
                                            "No discount codes yet."
                                        }
                                    }
                                }
                                if *loading.read() {
                                    tr {
                                        td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "9",
                                            "Loading discounts…"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    };

    rsx! {
        style { {PAGE_CSS} }
        div { class: "col", style: "height:100%",
            AdminNav {}
            div {
                class: ws.root_class(),
                tabindex: "0",
                onmousemove: move |e| ws.handle_mouse_move(&e),
                onmouseup: move |_| ws.handle_mouse_up(),
                {ws.render(body)}
                {ws.dock()}
            }
        }
    }
}

const PAGE_CSS: &str = "
.ad-workspace { flex: 1; min-height: 0; }
";
