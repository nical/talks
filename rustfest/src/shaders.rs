
pub static BACKGROUND_VERTEX: &'static str = &"
    #version 140
    in vec2 a_position;
    out vec2 v_position;

    void main() {
        gl_Position = vec4(a_position, 1.0, 1.0);
        v_position = a_position;
    }
";

pub static BACKGROUND_FRAGMENT: &'static str = &"
    #version 140
    uniform Globals {
        vec4 u_bg_color_1;
        vec4 u_bg_color_2;
        vec2 u_resolution;
        vec2 u_scroll_offset;
        float u_zoom;
        float u_blueprint;
    };
    in vec2 v_position;
    out vec4 out_color;

    void main() {
        vec2 px_position = v_position * vec2(1.0, -1.0) * u_resolution * 0.5;

        // #005fa4
        float vignette = clamp(0.7 * length(v_position), 0.0, 1.0);
        out_color = mix(
            u_bg_color_1,
            u_bg_color_2,
            vignette
        );

        // TODO: properly adapt the grid while zooming in and out.
        float grid_scale = 5.0;
        if (u_zoom < 2.5) {
            grid_scale = 1.0;
        }

        vec2 pos = px_position + u_scroll_offset * u_zoom;

        if (mod(pos.x, 20.0 / grid_scale * u_zoom) <= 1.0 ||
            mod(pos.y, 20.0 / grid_scale * u_zoom) <= 1.0) {
            out_color *= u_blueprint;
        }

        if (mod(pos.x, 100.0 / grid_scale * u_zoom) <= 3.0 ||
            mod(pos.y, 100.0 / grid_scale * u_zoom) <= 3.0) {
            out_color *= u_blueprint;
        }
    }
";

pub const PRIM_BUFFER_LEN: usize = 1024;

pub static VERTEX: &'static str = &"
    #version 140

    #define PRIM_BUFFER_LEN 1024

    uniform Globals {
        vec4 u_bg_color_1;
        vec4 u_bg_color_2;
        vec2 u_resolution;
        vec2 u_scroll_offset;
        float u_zoom;
        float u_blueprint;
    };

    struct Transform {
        vec4 data0;
        vec4 data1;
    };

    struct Primitive {
        vec4 color;
        vec2 user_data;
        int transform;
        int z_index;
    };

    uniform u_primitives { Primitive primitives[PRIM_BUFFER_LEN]; };
    uniform u_transforms { Transform transforms[PRIM_BUFFER_LEN]; };

    in vec2 a_position;
    in vec2 a_normal;
    in int a_prim_id;

    out vec4 v_color;

    void main() {
        int id = a_prim_id + gl_InstanceID;
        Primitive prim = primitives[id];

        Transform t = transforms[prim.transform];
        mat3 transform = mat3(
            t.data0.x, t.data0.y, 0.0,
            t.data0.z, t.data0.w, 0.0,
            t.data1.x, t.data1.y, 1.0
        );

        vec2 pos = a_position;

        pos = (transform * vec3(pos, 1.0)).xy;

        pos = pos - u_scroll_offset;

        pos = pos * u_zoom / (vec2(0.5, -0.5) * u_resolution);

        float z = float(prim.z_index) / 4096.0;
        gl_Position = vec4(pos, 1.0 - z, 1.0);
        v_color = prim.color;
    }
";

pub static FRAGMENT: &'static str = &"
    #version 140
    in vec4 v_color;
    out vec4 out_color;

    void main() {
        out_color = v_color;
    }
";
