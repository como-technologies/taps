use dioxus::prelude::*;
use tuesday_core::ReportConfig;

#[component]
pub fn TimePeriodConfig(config: Signal<ReportConfig>) -> Element {
    rsx! {
        div {
            div { class: "form-group",
                label { "Year:" }
                input {
                    r#type: "number",
                    value: "{config().year}",
                    min: "2020",
                    max: "2030",
                    oninput: move |evt| {
                        if let Ok(year) = evt.value().parse::<u32>() {
                            let mut cfg = config();
                            cfg.year = year;
                            config.set(cfg);
                        }
                    },
                }
            }
            div { class: "form-group",
                label { "Month:" }
                select {
                    value: "{config().month}",
                    onchange: move |evt| {
                        if let Ok(month) = evt.value().parse::<u32>() {
                            let mut cfg = config();
                            cfg.month = month;
                            config.set(cfg);
                        }
                    },
                    option { value: "1", "January" }
                    option { value: "2", "February" }
                    option { value: "3", "March" }
                    option { value: "4", "April" }
                    option { value: "5", "May" }
                    option { value: "6", "June" }
                    option { value: "7", "July" }
                    option { value: "8", "August" }
                    option { value: "9", "September" }
                    option { value: "10", "October" }
                    option { value: "11", "November" }
                    option { value: "12", "December" }
                }
            }
        }
    }
}
