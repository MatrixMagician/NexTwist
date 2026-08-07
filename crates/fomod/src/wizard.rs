//! The wizard **projection**: the authored `<moduleConfig>` AST flattened into the ordered
//! step/group/option tree a UI renders.
//!
//! This is the FOMOD spec's presentation half, and it belongs in the engine for the same
//! reason [`crate::resolve`] does: the `order` attribute on `<installSteps>`,
//! `<optionalFileGroups>` and `<plugins>` is a *spec rule*, not a UI preference. A shell
//! that re-implemented the sort could disagree with the engine about what "the second step"
//! means, which is exactly the class of drift the headless-engine boundary exists to stop.
//!
//! It is deliberately view-shaped and lossy: only what a wizard renders survives (names,
//! description, image path, authored type-state, condition flags). The install truth stays
//! with `resolve`, which the UI must still call as its dry-run gate before any write.

use crate::model::{FomodModule, GroupType, OrderKind, Plugin, PluginType};

/// The parsed module projected for a wizard: the module name plus its ordered steps.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WizardProjection {
    /// `<moduleName>` — the install's display name.
    pub module_name: String,
    /// The install steps in authored order.
    pub steps: Vec<WizardStep>,
}

/// One wizard install step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WizardStep {
    /// Step name.
    pub name: String,
    /// Whether the step carries a `<visible>` condition. Its live truth is decided by
    /// [`crate::resolve`] against the current flags; a wizard uses this only to know the
    /// step *may* disappear.
    pub conditional: bool,
    /// The option groups in this step, ordered.
    pub groups: Vec<WizardGroup>,
}

/// One option group within a step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WizardGroup {
    /// Group name.
    pub name: String,
    /// The selection constraint (radio vs checkbox, and the min/max rule).
    pub group_type: GroupType,
    /// The selectable options, ordered.
    pub options: Vec<WizardOption>,
}

/// One selectable option (`<plugin>`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WizardOption {
    /// Option name — also the selection identity's third element.
    pub name: String,
    /// `<description>` text.
    pub description: String,
    /// Archive-relative `<image path>` if the author supplied one.
    pub image: Option<String>,
    /// The **authored** type-state (static `<type>`, else the `<dependencyType>` default).
    /// The LIVE type after choices comes from [`crate::plugin_type_state`].
    pub default_type: PluginType,
    /// `(flag, value)` pairs this option sets when selected.
    pub flags: Vec<(String, String)>,
}

/// Project a parsed module into the ordered wizard tree, applying the `order` attribute at
/// each of the three levels the FOMOD spec defines it (steps, groups, plugins).
pub fn project(module: &FomodModule) -> WizardProjection {
    let mut steps = Vec::new();
    if let Some(step_list) = &module.steps {
        for step in ordered(&step_list.steps, step_list.order, |s| &s.name) {
            let mut groups = Vec::new();
            if let Some(group_list) = &step.groups {
                for group in ordered(&group_list.groups, group_list.order, |g| &g.name) {
                    let mut options = Vec::new();
                    if let Some(plugin_list) = &group.plugins {
                        for plugin in ordered(&plugin_list.plugins, plugin_list.order, |p| &p.name)
                        {
                            options.push(WizardOption {
                                name: plugin.name.clone(),
                                description: plugin.description.clone(),
                                image: plugin.image.as_ref().map(|i| i.path.clone()),
                                default_type: authored_type(plugin),
                                flags: plugin
                                    .condition_flags
                                    .as_ref()
                                    .map(|cf| {
                                        cf.flags
                                            .iter()
                                            .map(|f| (f.name.clone(), f.value.clone()))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            });
                        }
                    }
                    groups.push(WizardGroup {
                        name: group.name.clone(),
                        group_type: group.group_type,
                        options,
                    });
                }
            }
            steps.push(WizardStep {
                name: step.name.clone(),
                conditional: step.visible.is_some(),
                groups,
            });
        }
    }
    WizardProjection {
        module_name: module.module_name.clone(),
        steps,
    }
}

/// The authored default type-state of an option: the static `<type>`, else the
/// `<dependencyType>` default, else `Optional`.
///
/// Falling back to `Optional` (rather than `NotUsable`) is deliberate: a missing descriptor
/// must never silently disable an option in the wizard. `resolve` is the layer that rejects
/// a genuinely malformed `<typeDescriptor>` with a specific error.
pub fn authored_type(plugin: &Plugin) -> PluginType {
    match &plugin.type_descriptor {
        Some(td) => td
            .static_type
            .as_ref()
            .map(|t| t.name)
            .or_else(|| td.dependency_type.as_ref().map(|d| d.default_type.name))
            .unwrap_or(PluginType::Optional),
        None => PluginType::Optional,
    }
}

/// Apply a FOMOD `order` attribute: sort by name ascending/descending, or preserve document
/// order for `Explicit`. Returns references so no element is cloned to sort it.
fn ordered<T, F>(items: &[T], order: OrderKind, key: F) -> Vec<&T>
where
    F: Fn(&T) -> &String,
{
    let mut out: Vec<&T> = items.iter().collect();
    match order {
        OrderKind::Explicit => {}
        OrderKind::Ascending => out.sort_by(|a, b| key(a).cmp(key(b))),
        OrderKind::Descending => out.sort_by(|a, b| key(b).cmp(key(a))),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ConditionFlags, Group, GroupList, Image, InstallStep, PluginList, PluginTypeElem, SetFlag,
        StepList, TypeDescriptor,
    };

    fn plugin(name: &str) -> Plugin {
        Plugin {
            name: name.to_string(),
            description: String::new(),
            image: None,
            files: None,
            condition_flags: None,
            type_descriptor: Some(TypeDescriptor {
                static_type: Some(PluginTypeElem {
                    name: PluginType::Optional,
                }),
                dependency_type: None,
            }),
        }
    }

    fn group(name: &str, order: OrderKind, plugins: Vec<Plugin>) -> Group {
        Group {
            name: name.to_string(),
            group_type: GroupType::SelectAny,
            plugins: Some(PluginList { order, plugins }),
        }
    }

    fn step(name: &str, order: OrderKind, groups: Vec<Group>) -> InstallStep {
        InstallStep {
            name: name.to_string(),
            visible: None,
            groups: Some(GroupList { order, groups }),
        }
    }

    fn module(order: OrderKind, steps: Vec<InstallStep>) -> FomodModule {
        FomodModule {
            module_name: "Test".to_string(),
            module_deps: None,
            required: None,
            steps: Some(StepList { order, steps }),
            conditional: None,
        }
    }

    #[test]
    fn ascending_is_the_default_and_sorts_every_level_by_name() {
        let m = module(
            OrderKind::Ascending,
            vec![
                step(
                    "B step",
                    OrderKind::Ascending,
                    vec![group("Z", OrderKind::Ascending, vec![])],
                ),
                step(
                    "A step",
                    OrderKind::Ascending,
                    vec![
                        group("Y", OrderKind::Ascending, vec![]),
                        group("X", OrderKind::Ascending, vec![plugin("b"), plugin("a")]),
                    ],
                ),
            ],
        );
        let p = project(&m);
        assert_eq!(
            p.steps.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["A step", "B step"]
        );
        assert_eq!(
            p.steps[0]
                .groups
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>(),
            ["X", "Y"]
        );
        assert_eq!(
            p.steps[0].groups[0]
                .options
                .iter()
                .map(|o| o.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn explicit_preserves_document_order_and_descending_reverses_it() {
        let explicit = project(&module(
            OrderKind::Explicit,
            vec![
                step("B", OrderKind::Explicit, vec![]),
                step("A", OrderKind::Explicit, vec![]),
            ],
        ));
        assert_eq!(
            explicit
                .steps
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["B", "A"],
            "Explicit must not reorder the authored document"
        );

        let desc = project(&module(
            OrderKind::Descending,
            vec![
                step("A", OrderKind::Explicit, vec![]),
                step("B", OrderKind::Explicit, vec![]),
            ],
        ));
        assert_eq!(
            desc.steps
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["B", "A"]
        );
    }

    #[test]
    fn a_step_with_a_visible_condition_is_marked_conditional() {
        let mut s = step("Gated", OrderKind::Explicit, vec![]);
        s.visible = Some(Default::default());
        let p = project(&module(OrderKind::Explicit, vec![s]));
        assert!(p.steps[0].conditional);
    }

    #[test]
    fn option_flags_and_image_survive_the_projection() {
        let mut pl = plugin("Opt");
        pl.description = "does a thing".to_string();
        pl.image = Some(Image {
            path: "fomod/img.png".to_string(),
        });
        pl.condition_flags = Some(ConditionFlags {
            flags: vec![SetFlag {
                name: "colour".to_string(),
                value: "red".to_string(),
            }],
        });
        let p = project(&module(
            OrderKind::Explicit,
            vec![step(
                "S",
                OrderKind::Explicit,
                vec![group("G", OrderKind::Explicit, vec![pl])],
            )],
        ));
        let opt = &p.steps[0].groups[0].options[0];
        assert_eq!(opt.description, "does a thing");
        assert_eq!(opt.image.as_deref(), Some("fomod/img.png"));
        assert_eq!(
            opt.flags,
            vec![("colour".to_string(), "red".to_string())],
            "the wizard needs the authored flags to build the resolve selection"
        );
    }

    #[test]
    fn a_missing_type_descriptor_projects_optional_never_notusable() {
        // A wizard must not silently grey out an option because the author omitted the
        // descriptor; `resolve` is the layer that rejects it with a specific error.
        let mut pl = plugin("Opt");
        pl.type_descriptor = None;
        assert_eq!(authored_type(&pl), PluginType::Optional);
    }

    #[test]
    fn the_dependency_type_default_is_used_when_there_is_no_static_type() {
        use crate::model::DependencyType;
        let mut pl = plugin("Opt");
        pl.type_descriptor = Some(TypeDescriptor {
            static_type: None,
            dependency_type: Some(DependencyType {
                default_type: PluginTypeElem {
                    name: PluginType::Recommended,
                },
                patterns: None,
            }),
        });
        assert_eq!(authored_type(&pl), PluginType::Recommended);
    }

    #[test]
    fn a_module_with_no_steps_projects_an_empty_wizard() {
        let m = FomodModule {
            module_name: "Bare".to_string(),
            module_deps: None,
            required: None,
            steps: None,
            conditional: None,
        };
        let p = project(&m);
        assert_eq!(p.module_name, "Bare");
        assert!(p.steps.is_empty());
    }

    /// The serialized shape is the UI's wire contract (mirrored by `frontend/src/lib/api.ts`):
    /// snake_case field names, PascalCase enum variants, and `[name, value]` flag pairs.
    /// Pinned here because a rename would silently break the wizard rather than fail a build.
    #[test]
    fn the_serialized_wire_shape_is_stable() {
        let mut pl = plugin("Opt");
        pl.description = "d".to_string();
        pl.image = Some(Image {
            path: "img.png".to_string(),
        });
        pl.condition_flags = Some(ConditionFlags {
            flags: vec![SetFlag {
                name: "f".to_string(),
                value: "v".to_string(),
            }],
        });
        let p = project(&module(
            OrderKind::Explicit,
            vec![step(
                "S",
                OrderKind::Explicit,
                vec![group("G", OrderKind::Explicit, vec![pl])],
            )],
        ));
        let json = serde_json::to_string(&p).expect("serialize");
        assert_eq!(
            json,
            r#"{"module_name":"Test","steps":[{"name":"S","conditional":false,"groups":[{"name":"G","group_type":"SelectAny","options":[{"name":"Opt","description":"d","image":"img.png","default_type":"Optional","flags":[["f","v"]]}]}]}]}"#
        );
    }
}
