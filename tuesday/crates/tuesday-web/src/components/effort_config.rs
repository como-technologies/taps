use dioxus::prelude::*;
use tuesday_core::{ReportConfig, ScalingSeries};

#[component]
pub fn EffortConfig(config: Signal<ReportConfig>) -> Element {
    rsx! {
        div {
            div { class: "form-group",
                label { "Monthly Hours:" }
                input {
                    r#type: "number",
                    value: "{config().monthly_hours}",
                    step: "10",
                    min: "0",
                    oninput: move |evt| {
                        if let Ok(hours) = evt.value().parse::<f64>() {
                            let mut cfg = config();
                            cfg.monthly_hours = hours;
                            config.set(cfg);
                        }
                    },
                }
            }
            div { class: "form-group",
                label { "Effort Scaling:" }
                select {
                    value: "{config().scaling_series as u32}",
                    onchange: move |evt| {
                        let value = evt.value();
                        let mut cfg = config();
                        cfg.scaling_series = match value.as_str() {
                            "0" => ScalingSeries::Linear,
                            "1" => ScalingSeries::Doubling,
                            "2" => ScalingSeries::Fibonacci,
                            "3" => ScalingSeries::Exponential,
                            "4" => ScalingSeries::TShirtSizes,
                            "5" => ScalingSeries::Square,
                            _ => ScalingSeries::Linear,
                        };
                        config.set(cfg);
                    },
                    option { value: "0", "{ScalingSeries::Linear.description()}" }
                    option { value: "1", "{ScalingSeries::Doubling.description()}" }
                    option { value: "2", "{ScalingSeries::Fibonacci.description()}" }
                    option { value: "3", "{ScalingSeries::Exponential.description()}" }
                    option { value: "4", "{ScalingSeries::TShirtSizes.description()}" }
                    option { value: "5", "{ScalingSeries::Square.description()}" }
                }
            }
        }
    }
}
