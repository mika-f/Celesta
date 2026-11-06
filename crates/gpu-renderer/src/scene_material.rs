//! Scene materials' WGSL, checked against Celesta's material interface.
//!
//! A material's WGSL is assembled as custom shaders are (`shader.rs`):
//! `scene_material_prelude.wgsl`, the declarations of its texture slots,
//! the material's own code, then Celesta's entry points around the
//! functions the material defines. naga parses and validates the result on
//! the CPU; errors are located in the material's own code by line and
//! column, or name the interface when they lie in Celesta's declarations.

use crate::wgsl;
pub(crate) use celesta_plugin::scene::{
    MAX_MATERIAL_PARAMS, MAX_MATERIAL_TEXTURES, MAX_SHADER_BYTES,
};
use std::fmt::Write as _;

const PRELUDE: &str = include_str!("scene_material_prelude.wgsl");
const ENTRY: &str = include_str!("scene_material_entry.wgsl");
const OUTLINE_ENTRY: &str = include_str!("scene_material_outline_entry.wgsl");

/// Follows errors in the prelude, which the material completes.
const PRELUDE_HINT: &str = "(a material declares `struct Params`, the layout of its parameters)";

/// Follows errors in the entry points, which call the material.
const ENTRY_HINT: &str = "(a material defines `fn material_fragment(fragment: Fragment) -> vec4f` and may define `fn material_vertex(vertex: Vertex) -> VertexOffset`, `fn material_outline_fragment(fragment: Fragment) -> vec4f`, and `fn material_outline_vertex(vertex: Vertex) -> VertexOffset`)";

/// The vertex hook a material that defines none gets: no offset.
const DEFAULT_VERTEX: &str = "fn material_vertex(vertex: Vertex) -> VertexOffset {\n    return VertexOffset(vec3f(0.0), vec2f(0.0));\n}\n";

/// The outline's vertex hook a material with an outline that defines none
/// gets: no offset.
const DEFAULT_OUTLINE_VERTEX: &str = "fn material_outline_vertex(vertex: Vertex) -> VertexOffset {\n    return VertexOffset(vec3f(0.0), vec2f(0.0));\n}\n";

/// A material's module, checked.
#[derive(Debug)]
pub(crate) struct CheckedMaterial {
    /// The prelude, the slots, the material's code, and the entry points.
    pub(crate) module: naga::Module,
    /// naga's layout size of the material's `Params`.
    pub(crate) params_size: u32,
    /// Whether the material defines `material_outline_fragment`, and so the
    /// module the outline pass's entry points.
    pub(crate) outline: bool,
}

/// Assembles and checks a material's `wgsl` with `textures` slots, or says
/// why it breaks the interface. CPU only.
pub(crate) fn check_material(wgsl: &str, textures: usize) -> Result<CheckedMaterial, String> {
    if wgsl.len() > MAX_SHADER_BYTES {
        return Err(format!(
            "is {} bytes of WGSL, more than {MAX_SHADER_BYTES}",
            wgsl.len()
        ));
    }
    if textures > MAX_MATERIAL_TEXTURES {
        return Err(format!(
            "has {textures} texture slots, more than {MAX_MATERIAL_TEXTURES}"
        ));
    }
    let mut code = String::from(PRELUDE);
    for slot in 0..textures {
        write!(
            code,
            "\n@group(2) @binding({})\nvar texture_{slot}: texture_2d<f32>;\n@group(2) @binding({})\nvar sampler_{slot}: sampler;\n",
            2 * slot,
            2 * slot + 1
        )
        .expect("writing to a String cannot fail");
    }
    code.push('\n');
    let start = code.len();
    code.push_str(wgsl);
    let author = start..code.len();
    // Both parses below keep the material's code at `author`, and `locate`
    // reads only that range of `code`, so it serves both.
    let locate = |at: Option<usize>, message: String| {
        wgsl::locate(&code, &author, at, message, PRELUDE_HINT, ENTRY_HINT)
    };

    // naga resolves a name to the module's own declaration before a
    // built-in, so a material redeclaring one would change what Celesta's
    // code around it calls. Checked before parsing, since such a
    // declaration can break the prelude's code first.
    let shadows = |at: usize, name: &str| {
        Err(locate(
            Some(at),
            format!(
                "declares `{name}`, which WGSL predeclares; a material cannot redeclare WGSL's built-in functions and types"
            ),
        ))
    };
    for (offset, name) in wgsl::module_declarations(wgsl) {
        if wgsl::predeclared(name) {
            return shadows(author.start + offset, name);
        }
    }

    // The material's code with the prelude stands alone: the entry points
    // call into it, not the other way round.
    let parse = |code: &str| {
        naga::front::wgsl::parse_str(code)
            .map_err(|error| locate(wgsl::parse_error_at(&error), error.message().to_owned()))
    };
    let alone = parse(&code)?;
    wgsl::check_declarations(
        &alone,
        &code,
        &author,
        &wgsl::Rules {
            what: "a material",
            prelude_resource: &|name| prelude_resource(name, textures),
            entry_points: &[],
            instead: "a material defines `material_fragment` and Celesta's entry points call it",
        },
    )?;
    // The pipelines Celesta makes for a material set only the constants of
    // Celesta's entry points, which `alone` does not hold.
    if let Some((handle, constant)) = alone.overrides.iter().next() {
        let at = alone
            .overrides
            .get_span(handle)
            .to_range()
            .map(|range| range.start);
        return Err(locate(
            at,
            format!(
                "declares the override `{}`; a material cannot declare pipeline-overridable constants",
                constant.name.as_deref().unwrap_or("")
            ),
        ));
    }
    let defines = |name: &str| {
        alone
            .functions
            .iter()
            .any(|(_, function)| function.name.as_deref() == Some(name))
    };
    if !defines("material_fragment") {
        return Err("defines no `fn material_fragment(fragment: Fragment) -> vec4f`".to_owned());
    }
    let outline = defines("material_outline_fragment");
    if defines("material_outline_vertex") && !outline {
        return Err(
            "defines `material_outline_vertex` but no `material_outline_fragment`".to_owned(),
        );
    }

    let mut assembled = code.clone();
    assembled.push('\n');
    if !defines("material_vertex") {
        assembled.push_str(DEFAULT_VERTEX);
    }
    if outline && !defines("material_outline_vertex") {
        assembled.push_str(DEFAULT_OUTLINE_VERTEX);
    }
    assembled.push_str(ENTRY);
    if outline {
        assembled.push('\n');
        assembled.push_str(OUTLINE_ENTRY);
    }
    let module = parse(&assembled)?;
    // The same checks against naga's own reading of the material, should
    // the source scan above ever read a declaration differently. naga
    // keeps no alias names and no abstract constants, so those are the
    // scan's alone (see `wgsl::module_names`).
    for (name, at) in wgsl::module_names(&module, &author) {
        if wgsl::predeclared(name) {
            return shadows(at, name);
        }
        if wgsl::reserved(name) {
            return Err(locate(
                Some(at),
                format!("declares `{name}`; names starting with `celesta` belong to Celesta"),
            ));
        }
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|error| {
        let mut message = wgsl::error_chain(error.as_inner());
        // naga gives this error no span: it lies somewhere in the vertex
        // hooks the entry point calls.
        if let naga::valid::ValidationError::EntryPoint {
            stage: naga::ShaderStage::Vertex,
            source: naga::valid::EntryPointError::ForbiddenStageOperations,
            ..
        } = error.as_inner()
        {
            message.push_str(
                " (vertex hooks sample with `textureSampleLevel`, and take no derivatives)",
            );
        }
        locate(wgsl::validation_error_at(&error), message)
    })?;

    let params_size = params_size(&module)?;
    if params_size as usize > MAX_MATERIAL_PARAMS {
        return Err(format!(
            "Params is {params_size} bytes, more than {MAX_MATERIAL_PARAMS}"
        ));
    }
    Ok(CheckedMaterial {
        module,
        params_size,
        outline,
    })
}

/// Whether the prelude or the slots of a material with `textures` slots
/// declare the resource `name`.
fn prelude_resource(name: &str, textures: usize) -> bool {
    let slot = |prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(|slot| slot.parse::<usize>().ok())
            .is_some_and(|slot| slot < textures && name == format!("{prefix}{slot}"))
    };
    matches!(name, "frame" | "draw" | "params") || slot("texture_") || slot("sampler_")
}

/// The size of the type of `module`'s `params`, which must be a struct.
fn params_size(module: &naga::Module) -> Result<u32, String> {
    let (_, params) = module
        .global_variables
        .iter()
        .find(|(_, global)| global.name.as_deref() == Some("params"))
        .expect("the prelude declares `params`");
    if !matches!(
        module.types[params.ty].inner,
        naga::TypeInner::Struct { .. }
    ) {
        return Err(format!("`Params` is not a struct {PRELUDE_HINT}"));
    }
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|error| wgsl::error_chain(&error))?;
    Ok(layouter[params.ty].size)
}

#[cfg(test)]
mod tests {
    use super::check_material;

    const FRAGMENT: &str =
        "fn material_fragment(fragment: Fragment) -> vec4f {\n    return params.color;\n}\n";

    fn with_params(body: &str) -> String {
        format!("struct Params {{ color: vec4f }};\n{body}")
    }

    #[test]
    fn minimal_material_checks() {
        let checked = check_material(&with_params(FRAGMENT), 0).unwrap();
        assert_eq!(checked.params_size, 16);
        assert!(!checked.outline);
        // Celesta's entry points declare the one override the pipelines
        // set, which a material cannot.
        let overrides: Vec<_> = checked
            .module
            .overrides
            .iter()
            .map(|(_, constant)| constant.name.as_deref())
            .collect();
        assert_eq!(overrides, [Some("CELESTA_SAMPLES")]);
    }

    #[test]
    fn params_size_follows_the_wgsl_layout() {
        let size = |params: &str| {
            let wgsl = format!(
                "struct Params {{ {params} }};\nfn material_fragment(fragment: Fragment) -> vec4f {{\n    return vec4f(1.0);\n}}\n"
            );
            check_material(&wgsl, 0).unwrap().params_size
        };
        assert_eq!(size("a: f32, b: vec3f"), 32);
        assert_eq!(size("a: vec4f, b: array<vec4f, 4>"), 80);
    }

    #[test]
    fn outline_functions_make_an_outline() {
        let outline = "fn material_outline_fragment(fragment: Fragment) -> vec4f {\n    return vec4f(0.0, 0.0, 0.0, 1.0);\n}\n";
        let checked = check_material(&with_params(&format!("{FRAGMENT}{outline}")), 0).unwrap();
        assert!(checked.outline);

        let vertex = "fn material_outline_vertex(vertex: Vertex) -> VertexOffset {\n    return VertexOffset(vertex.normal * 0.01, vec2f(0.0));\n}\n";
        let checked =
            check_material(&with_params(&format!("{FRAGMENT}{outline}{vertex}")), 0).unwrap();
        assert!(checked.outline);

        let error = check_material(&with_params(&format!("{FRAGMENT}{vertex}")), 0).unwrap_err();
        assert!(error.contains("no `material_outline_fragment`"), "{error}");
    }

    #[test]
    fn broken_wgsl_is_located_in_the_plugins_source() {
        let wgsl = "struct Params { color: vec4f };\nfn material_fragment(fragment: Fragment) -> vec4f {\n  return nope;\n}\n";
        let error = check_material(wgsl, 0).unwrap_err();
        assert!(error.starts_with("3:10:"), "{error}");
        assert!(error.contains("nope"), "{error}");
    }

    #[test]
    fn wrong_signatures_name_the_interface() {
        let error = check_material(
            &with_params(
                "fn material_fragment(fragment: Fragment) -> vec3f {\n    return params.color.rgb;\n}\n",
            ),
            0,
        )
        .unwrap_err();
        assert!(
            error.contains("fn material_fragment(fragment: Fragment) -> vec4f"),
            "{error}"
        );

        let error = check_material(
            &with_params(&format!(
                "{FRAGMENT}fn material_vertex(vertex: Vertex) -> vec3f {{\n    return vertex.position;\n}}\n"
            )),
            0,
        )
        .unwrap_err();
        assert!(
            error.contains("fn material_vertex(vertex: Vertex) -> VertexOffset"),
            "{error}"
        );
    }

    #[test]
    fn missing_parts_are_refused() {
        let error = check_material(&with_params("fn other() {}\n"), 0).unwrap_err();
        assert!(error.contains("defines no"), "{error}");

        let error = check_material(
            "fn material_fragment(fragment: Fragment) -> vec4f {\n    return vec4f(1.0);\n}\n",
            0,
        )
        .unwrap_err();
        assert!(error.contains("struct Params"), "{error}");

        let error = check_material(
            "alias Params = vec4f;\nfn material_fragment(fragment: Fragment) -> vec4f {\n    return params;\n}\n",
            0,
        )
        .unwrap_err();
        assert!(error.contains("`Params` is not a struct"), "{error}");
    }

    #[test]
    fn own_resources_entry_points_and_celesta_names_are_refused() {
        let error = check_material(
            &with_params(&format!(
                "@group(3) @binding(0) var<uniform> mine: vec4f;\n{FRAGMENT}"
            )),
            0,
        )
        .unwrap_err();
        assert!(error.contains("declares the resource `mine`"), "{error}");
        assert!(error.starts_with("2:"), "{error}");

        // A prelude's name declared again by the material.
        let error = check_material(
            &with_params(&format!(
                "@group(2) @binding(0) var texture_0: texture_2d<f32>;\n{FRAGMENT}"
            )),
            0,
        )
        .unwrap_err();
        assert!(
            error.contains("declares the resource `texture_0`"),
            "{error}"
        );

        let error = check_material(
            &with_params(&format!(
                "@fragment fn fs() -> @location(0) vec4f {{\n    return vec4f(1.0);\n}}\n{FRAGMENT}"
            )),
            0,
        )
        .unwrap_err();
        assert!(error.contains("declares the entry point `fs`"), "{error}");
        assert!(error.starts_with("2:"), "{error}");

        let error = check_material(&with_params(&format!("{FRAGMENT}fn celesta_x() {{}}\n")), 0)
            .unwrap_err();
        assert!(error.contains("belong to Celesta"), "{error}");
        assert!(error.starts_with("5:"), "{error}");
    }

    #[test]
    fn vertex_hooks_cannot_sample_with_implicit_lod() {
        let material = |sample: &str| {
            with_params(&format!(
                "{FRAGMENT}fn material_outline_fragment(fragment: Fragment) -> vec4f {{\n    return params.color;\n}}\nfn material_outline_vertex(vertex: Vertex) -> VertexOffset {{\n    let width = {sample}.g;\n    return VertexOffset(vertex.normal * width, vec2f(0.0));\n}}\n"
            ))
        };
        let error = check_material(
            &material("textureSample(texture_0, sampler_0, vertex.uv)"),
            1,
        )
        .unwrap_err();
        assert!(error.contains("celesta_outline_vertex"), "{error}");
        assert!(
            error.contains("vertex hooks sample with `textureSampleLevel`"),
            "{error}"
        );
        let checked = check_material(
            &material("textureSampleLevel(texture_0, sampler_0, vertex.uv, 0.0)"),
            1,
        )
        .unwrap();
        assert!(checked.outline);

        let surface = |sample: &str| {
            with_params(&format!(
                "{FRAGMENT}fn material_vertex(vertex: Vertex) -> VertexOffset {{\n    let width = {sample}.g;\n    return VertexOffset(vertex.normal * width, vec2f(0.0));\n}}\n"
            ))
        };
        let error = check_material(
            &surface("textureSample(texture_0, sampler_0, vertex.uv)"),
            1,
        )
        .unwrap_err();
        assert!(error.contains("celesta_surface_vertex"), "{error}");
        assert!(
            error.contains("vertex hooks sample with `textureSampleLevel`"),
            "{error}"
        );
        assert!(
            check_material(
                &surface("textureSampleLevel(texture_0, sampler_0, vertex.uv, 0.0)"),
                1
            )
            .is_ok()
        );
    }

    #[test]
    fn built_in_names_are_refused() {
        for (declaration, name) in [
            (
                "fn clamp(e: f32, low: f32, high: f32) -> f32 {\n    return e;\n}\n",
                "clamp",
            ),
            ("const select = 1.0;\n", "select"),
            ("override fwidth: f32 = 1.0;\n", "fwidth"),
            ("var<private> saturate: f32;\n", "saturate"),
            ("struct dot { a: f32 };\n", "dot"),
            ("alias inverseSqrt = f32;\n", "inverseSqrt"),
            ("alias vec4f = vec3f;\n", "vec4f"),
            ("struct mat3x3h { a: f32 };\n", "mat3x3h"),
            ("alias texture_2d = f32;\n", "texture_2d"),
            ("const f32 = 1;\n", "f32"),
            ("fn textureSampleLevel() {}\n", "textureSampleLevel"),
        ] {
            let error =
                check_material(&with_params(&format!("{FRAGMENT}{declaration}")), 0).unwrap_err();
            assert!(error.starts_with("5:"), "{declaration}: {error}");
            assert!(
                error.contains(&format!("declares `{name}`, which WGSL predeclares")),
                "{declaration}: {error}"
            );
        }

        // Names that only start like a built-in's, comments, and a
        // function's own locals are the material's.
        let own = "// fn clamp\n/* alias dot = f32; /* nested */ fn max */\nfn clamped(value: f32) -> f32 {\n    let min = 0.0;\n    return clamp(value, min, 1.0);\n}\nconst texture_scale = 2.0;\nstruct vec4data { a: f32 };\n";
        check_material(&with_params(&format!("{FRAGMENT}{own}")), 0).unwrap();

        // Words split where naga splits them: `a‿fn` (U+203F joins words)
        // is one name, so `min` after it is still declared.
        let joined =
            "const a\u{203f}fn = 1.0;\nfn min(a: f32, b: f32) -> f32 {\n    return a;\n}\n";
        let error = check_material(&with_params(&format!("{FRAGMENT}{joined}")), 0).unwrap_err();
        assert!(error.starts_with("6:"), "{error}");
        assert!(
            error.contains("declares `min`, which WGSL predeclares"),
            "{error}"
        );
        // A line comment ends at every line break naga knows, not only at
        // `\n`.
        for line_break in ["\r", "\u{b}", "\u{c}", "\u{85}", "\u{2028}", "\u{2029}"] {
            let after_comment =
                format!("// x{line_break}fn min(a: f32, b: f32) -> f32 {{\n    return a;\n}}\n");
            let error =
                check_material(&with_params(&format!("{FRAGMENT}{after_comment}")), 0).unwrap_err();
            assert!(error.starts_with("5:"), "{line_break:?}: {error}");
            assert!(
                error.contains("declares `min`, which WGSL predeclares"),
                "{line_break:?}: {error}"
            );
        }
        // A combining mark makes another name than the built-in's.
        let marked = "fn min\u{301}(a: f32, b: f32) -> f32 {\n    return a;\n}\n";
        check_material(&with_params(&format!("{FRAGMENT}{marked}")), 0).unwrap();
    }

    #[test]
    fn overrides_are_refused() {
        let error = check_material(
            &with_params(&format!("{FRAGMENT}override k: f32 = 1.0;\n")),
            0,
        )
        .unwrap_err();
        assert!(error.starts_with("5:"), "{error}");
        assert!(error.contains("declares the override `k`"), "{error}");

        let error =
            check_material(&with_params(&format!("{FRAGMENT}override k: f32;\n")), 0).unwrap_err();
        assert!(error.contains("declares the override `k`"), "{error}");

        let error = check_material(
            &with_params(&format!("{FRAGMENT}override celesta_k: f32 = 1.0;\n")),
            0,
        )
        .unwrap_err();
        assert!(error.contains("`celesta_k`"), "{error}");
        assert!(error.contains("belong to Celesta"), "{error}");
    }

    #[test]
    fn workgroup_variables_are_refused() {
        // naga's validator lets any stage use workgroup memory, which WGSL
        // allows compute shaders alone; used or not, it is refused.
        for body in [
            "",
            "fn material_vertex(vertex: Vertex) -> VertexOffset {\n    w[0] = 1.0;\n    return VertexOffset(vec3f(w[0]), vec2f(0.0));\n}\n",
        ] {
            let error = check_material(
                &with_params(&format!(
                    "{FRAGMENT}var<workgroup> w: array<f32, 4096>;\n{body}"
                )),
                0,
            )
            .unwrap_err();
            assert!(error.starts_with("5:"), "{error}");
            assert!(
                error.contains("declares the workgroup variable `w`"),
                "{error}"
            );
            assert!(error.contains("compute shaders"), "{error}");
        }
    }

    #[test]
    fn celesta_names_are_refused_in_any_case() {
        for (declaration, name) in [
            ("struct CelestaVarying { a: f32 };\n", "CelestaVarying"),
            ("alias CELESTA_X = f32;\n", "CELESTA_X"),
            ("const Celesta = 1.0;\n", "Celesta"),
        ] {
            let error =
                check_material(&with_params(&format!("{FRAGMENT}{declaration}")), 0).unwrap_err();
            assert!(error.starts_with("5:"), "{declaration}: {error}");
            assert!(
                error.contains(&format!("declares `{name}`; names starting with `celesta`")),
                "{declaration}: {error}"
            );
        }
    }

    #[test]
    fn textures_are_declared_by_slot_count() {
        let reading = |texture: &str| {
            with_params(&format!(
                "fn material_fragment(fragment: Fragment) -> vec4f {{\n    return params.color * textureSample({texture}, sampler_0, fragment.uv);\n}}\n"
            ))
        };
        assert!(check_material(&reading("texture_1"), 2).is_ok());
        let error = check_material(&reading("texture_2"), 2).unwrap_err();
        assert!(error.contains("texture_2"), "{error}");
        assert!(error.starts_with("3:"), "{error}");
    }

    #[test]
    fn limits_are_refused() {
        let mut wgsl = with_params(FRAGMENT);
        wgsl.push_str(&" ".repeat(262_145 - wgsl.len()));
        assert_eq!(wgsl.len(), 262_145);
        let error = check_material(&wgsl, 0).unwrap_err();
        assert!(error.contains("more than 262144"), "{error}");
        wgsl.pop();
        assert!(check_material(&wgsl, 0).is_ok());

        let error = check_material(&with_params(FRAGMENT), 13).unwrap_err();
        assert!(error.contains("more than 12"), "{error}");
        assert!(check_material(&with_params(FRAGMENT), 12).is_ok());

        let params = |count: usize| {
            format!(
                "struct Params {{ a: array<vec4f, {count}> }};\nfn material_fragment(fragment: Fragment) -> vec4f {{\n    return params.a[0];\n}}\n"
            )
        };
        let error = check_material(&params(33), 0).unwrap_err();
        assert!(error.contains("528 bytes, more than 512"), "{error}");
        assert_eq!(check_material(&params(32), 0).unwrap().params_size, 512);
    }
}
