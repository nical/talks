#[macro_use]
extern crate gfx;
extern crate gfx_window_glutin;
extern crate gfx_device_gl;
extern crate glutin;
extern crate lyon;
extern crate resvg;

//extern crate ron;
//extern crate serde;
//#[macro_use]
//extern crate serde_derive;

mod shaders;

use lyon::path::builder::*;
use lyon::math::*;
use lyon::geom::euclid::Transform2D;
use lyon::tessellation::geometry_builder::{VertexConstructor, VertexBuffers, BuffersBuilder};
use lyon::tessellation::basic_shapes::*;
use lyon::tessellation::{FillTessellator, FillOptions};
use lyon::tessellation::{StrokeTessellator, StrokeOptions};
use lyon::tessellation;
use lyon::path::default::Path;
use lyon::svg::path_utils::build_path;

use gfx::traits::{Device, FactoryExt};
use glutin::{GlContext, EventsLoop, KeyboardInput};
use glutin::ElementState::Pressed;
use resvg::tree::{self, Color};

const DEFAULT_WINDOW_WIDTH: f32 = 800.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;

const FALLBACK_COLOR: Color = Color {
    red: 0,
    green: 0,
    blue: 0,
};

fn main() {
    let mut slides = load_slides();

    let mut bg_geometry: VertexBuffers<BgVertex> = VertexBuffers::new();

    fill_rectangle(
        &Rect::new(point(-1.0, -1.0), size(2.0, 2.0)),
        &FillOptions::default(),
        &mut BuffersBuilder::new(&mut bg_geometry, BgVertexCtor),
    );

    let glutin_builder = glutin::WindowBuilder::new()
        //.with_dimensions(DEFAULT_WINDOW_WIDTH as u32, DEFAULT_WINDOW_HEIGHT as u32)
        .with_maximized(true)
        .with_decorations(false)
        .with_title("rustfest".to_string());

    let mut events_loop = glutin::EventsLoop::new();
    let context = glutin::ContextBuilder::new()
        .with_multisampling(8)
        .with_vsync(true);
    let (window, mut device, mut factory, mut main_fbo, mut main_depth) =
        gfx_window_glutin::init::<gfx::format::Rgba8, gfx::format::DepthStencil>(glutin_builder, context, &events_loop);

    let bg_pso = factory.create_pipeline_simple(
        shaders::BACKGROUND_VERTEX.as_bytes(),
        shaders::BACKGROUND_FRAGMENT.as_bytes(),
        bg_pipeline::new(),
    ).unwrap();

    let path_shader = factory.link_program(
        shaders::VERTEX.as_bytes(),
        shaders::FRAGMENT.as_bytes()
    ).unwrap();

    let mut rasterizer_state = gfx::state::Rasterizer::new_fill().with_cull_back();
    rasterizer_state.samples = Some(gfx::state::MultiSample);
    let path_pso = factory.create_pipeline_from_program(
        &path_shader,
        gfx::Primitive::TriangleList,
        rasterizer_state,
        path_pipeline::new(),
    ).unwrap();

    let mut wireframe_fill_mode = gfx::state::Rasterizer::new_fill();
    wireframe_fill_mode.method = gfx::state::RasterMethod::Line(1);
    let wireframe_pso = factory.create_pipeline_from_program(
        &path_shader,
        gfx::Primitive::TriangleList,
        wireframe_fill_mode,
        path_pipeline::new(),
    ).unwrap();

    let (bg_vbo, bg_range) = factory.create_vertex_buffer_with_slice(
        &bg_geometry.vertices[..],
        &bg_geometry.indices[..]
    );

    let viewbox_w = 297.0;
    let viewbox_h = 210.0;
    let gpu_globals = factory.create_constant_buffer(1);

    let (w, h) = window.get_inner_size().unwrap();
    let zoom = (h as f32 / viewbox_h).min(w as f32 / viewbox_w);
    let scroll_x = viewbox_w * 0.5;
    let scroll_y = viewbox_h * 0.5;

    let mut view = ViewParams {
        target_zoom: zoom,
        default_zoom: zoom,
        zoom: 10.0,
        target_scroll: vector(scroll_x, scroll_y),
        default_scroll: vector(scroll_x, scroll_y),
        scroll: vector(70.0, 70.0),
        wireframe: false,
        stroke_width: 1.0,
        target_stroke_width: 1.0,
        window_size: (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
        camera_mode: false,
        current_slide: 0,
        reload: false,
        calibrate: false,
    };

    let mut cmd_queue: CommandQueue = factory.create_command_buffer().into();

    let mut slide = view.current_slide;
    let mut gpu_scene = GpuScene::new(&slides[slide], &mut factory, &mut cmd_queue);

    let camera_mode_color_1 = [0.0, 0.47, 0.9, 1.0];
    let camera_mode_color_2 = [0.0, 0.1, 0.64, 1.0];
    let mut bg_color_1 = [0.0, 0.47, 0.9, 1.0];
    let mut bg_color_2 = [0.0, 0.1, 0.64, 1.0];
    let mut blueprint = 1.3;

    while update_inputs(&mut events_loop, &mut view) {
        if view.reload {
            slides = load_slides();
        }

        if view.current_slide >= slides.len() {
            view.current_slide = slides.len() - 1;
        }
        if view.reload || view.current_slide != slide {
            slide = view.current_slide;
            gpu_scene = GpuScene::new(&slides[slide], &mut factory, &mut cmd_queue);
        }

        view.reload = false;
        if view.calibrate {
            bg_color_1 = [1.0, 1.0, 1.0, 1.0];
            bg_color_2 = [1.0, 1.0, 1.0, 1.0];
            view.calibrate = false;
        }

        let (target_color_1, target_color_2, target_blueprint) = if view.camera_mode {
            (camera_mode_color_1, camera_mode_color_2, 1.3)
        } else {
            (slides[slide].color_1, slides[slide].color_2, slides[slide].blueprint)
        };

        blueprint = blueprint + 0.02 * (target_blueprint - blueprint);
        for i in 0..4 {
            bg_color_1[i] = bg_color_1[i] + 0.07 * (target_color_1[i] - bg_color_1[i]);
            bg_color_2[i] = bg_color_2[i] + 0.07 * (target_color_2[i] - bg_color_2[i]);
        }

        gfx_window_glutin::update_views(&window, &mut main_fbo, &mut main_depth);
        let (w, h) = window.get_inner_size().unwrap();
        view.window_size = (w as f32, h as f32);

        cmd_queue.clear(&main_fbo.clone(), [0.0, 0.0, 0.0, 0.0]);
        cmd_queue.clear_depth(&main_depth.clone(), 1.0);

        cmd_queue.update_constant_buffer(
            &gpu_globals,
            &Globals {
                resolution: [w as f32, h as f32],
                zoom: view.zoom,
                scroll_offset: view.scroll.to_array(),
                bg_color_1,
                bg_color_2,
                blueprint,
            },
        );

        let pso = if view.wireframe { &wireframe_pso } else { &path_pso };

        cmd_queue.draw(
            &gpu_scene.range,
            &pso,
            &path_pipeline::Data {
                vbo: gpu_scene.geometry.clone(),
                primitives: gpu_scene.primitives.clone(),
                transforms: gpu_scene.transforms.clone(),
                gpu_globals: gpu_globals.clone(),
                out_color: main_fbo.clone(),
                out_depth: main_depth.clone(),
            },
        );

        cmd_queue.draw(
            &bg_range,
            &bg_pso,
            &bg_pipeline::Data {
                vbo: bg_vbo.clone(),
                out_color: main_fbo.clone(),
                out_depth: main_depth.clone(),
                gpu_globals: gpu_globals.clone(),
            },
        );

        cmd_queue.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

fn convert_path(svg_path: &tree::Path) -> Path {
    fn p(x: f64, y: f64) -> Point { Point::new(x as f32, y as f32) }

    let mut path = Path::builder();
    for segment in &svg_path.segments {
        match *segment {
            tree::PathSegment::MoveTo { x, y } => { path.move_to(p(x, y)); },
            tree::PathSegment::LineTo { x, y } => { path.line_to(p(x, y)); },
            tree::PathSegment::CurveTo { x1, y1, x2, y2, x, y, } => {
                path.cubic_bezier_to(p(x1, y1), p(x2, y2), p(x, y));
            }
            tree::PathSegment::ClosePath => { path.close(); }
        }
    }
    path.build()
}

fn convert_color(paint: &tree::Paint, opacity: f64) -> [f32; 4] {
    let color = match paint {
        &resvg::tree::Paint::Color(c) => c,
        _ => FALLBACK_COLOR,
    };

    [
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        opacity as f32,
    ]
}

gfx_defines!{
    constant Globals {
        bg_color_1: [f32; 4] = "u_bg_color_1",
        bg_color_2: [f32; 4] = "u_bg_color_2",
        resolution: [f32; 2] = "u_resolution",
        scroll_offset: [f32; 2] = "u_scroll_offset",
        zoom: f32 = "u_zoom",
        blueprint: f32 = "u_blueprint",
    }

    vertex GpuVertex {
        position: [f32; 2] = "a_position",
        normal: [f32; 2] = "a_normal",
        prim_id: i32 = "a_prim_id", // An id pointing to the PrimData struct above.
    }

    constant Primitive {
        color: [f32; 4] = "color",
        user_data: [f32; 2] = "user_data",
        transform: i32 = "transform",
        z_index: i32 = "z_index",
    }

    constant Transform {
        data0: [f32; 4] = "data0",
        data1: [f32; 4] = "data1",
    }

    pipeline path_pipeline {
        vbo: gfx::VertexBuffer<GpuVertex> = (),
        out_color: gfx::RenderTarget<gfx::format::Rgba8> = "out_color",
        out_depth: gfx::DepthTarget<gfx::format::DepthStencil> = gfx::preset::depth::LESS_EQUAL_WRITE,
        gpu_globals: gfx::ConstantBuffer<Globals> = "Globals",
        primitives: gfx::ConstantBuffer<Primitive> = "u_primitives",
        transforms: gfx::ConstantBuffer<Transform> = "u_transforms",
    }

    vertex BgVertex {
        position: [f32; 2] = "a_position",
    }

    pipeline bg_pipeline {
        vbo: gfx::VertexBuffer<BgVertex> = (),
        out_color: gfx::RenderTarget<gfx::format::Rgba8> = "out_color",
        out_depth: gfx::DepthTarget<gfx::format::DepthStencil> = gfx::preset::depth::LESS_EQUAL_WRITE,
        gpu_globals: gfx::ConstantBuffer<Globals> = "Globals",
    }
}

struct BgVertexCtor;
impl VertexConstructor<tessellation::FillVertex, BgVertex> for BgVertexCtor {
    fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> BgVertex {
        BgVertex { position: vertex.position.to_array() }
    }
}

/// This vertex constructor forwards the positions and normals provided by the
/// tessellators and add a shape id.
pub struct WithId(pub i32);

impl VertexConstructor<tessellation::FillVertex, GpuVertex> for WithId {
    fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> GpuVertex {
        debug_assert!(!vertex.position.x.is_nan());
        debug_assert!(!vertex.position.y.is_nan());
        debug_assert!(!vertex.normal.x.is_nan());
        debug_assert!(!vertex.normal.y.is_nan());
        GpuVertex {
            position: vertex.position.to_array(),
            normal: vertex.normal.to_array(),
            prim_id: self.0,
        }
    }
}

impl VertexConstructor<tessellation::StrokeVertex, GpuVertex> for WithId {
    fn new_vertex(&mut self, vertex: tessellation::StrokeVertex) -> GpuVertex {
        debug_assert!(!vertex.position.x.is_nan());
        debug_assert!(!vertex.position.y.is_nan());
        debug_assert!(!vertex.normal.x.is_nan());
        debug_assert!(!vertex.normal.y.is_nan());
        debug_assert!(!vertex.advancement.is_nan());
        GpuVertex {
            position: vertex.position.to_array(),
            normal: vertex.normal.to_array(),
            prim_id: self.0,
        }
    }
}

struct ViewParams {
    target_zoom: f32,
    default_zoom: f32,
    zoom: f32,
    target_scroll: Vector,
    default_scroll: Vector,
    scroll: Vector,
    wireframe: bool,
    stroke_width: f32,
    target_stroke_width: f32,
    window_size: (f32, f32),
    camera_mode: bool,
    current_slide: usize,
    reload: bool,
    calibrate: bool,
}

fn update_inputs(
    events_loop: &mut EventsLoop,
    view: &mut ViewParams
) -> bool {
    use glutin::Event;
    use glutin::VirtualKeyCode;
    use glutin::WindowEvent;

    let mut status = true;

    events_loop.poll_events(|event| {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Closed,
                ..
            } => {
                status = false;
            },
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input: KeyboardInput {
                        state: Pressed,
                        virtual_keycode: Some(key),
                        ..
                    },
                    ..
                },
                ..
            } => {
                match key {
                    VirtualKeyCode::Escape => {
                        status = false;
                    }
                    VirtualKeyCode::PageDown => {
                        view.camera_mode = true;
                        view.target_zoom *= 0.8;
                    }
                    VirtualKeyCode::PageUp => {
                        view.camera_mode = true;
                        view.target_zoom *= 1.25;
                    }
                    VirtualKeyCode::Left => {
                        if view.camera_mode {
                            view.target_scroll.x -= 50.0 / view.target_zoom;
                        } else if view.current_slide > 0 {
                            view.current_slide -= 1;
                        }
                    }
                    VirtualKeyCode::Right => {
                        if view.camera_mode {
                            view.target_scroll.x += 50.0 / view.target_zoom;
                        } else {
                            view.current_slide += 1;
                        }
                    }
                    VirtualKeyCode::Up => {
                        if view.camera_mode {
                            view.target_scroll.y -= 50.0 / view.target_zoom;
                        }
                    }
                    VirtualKeyCode::Down => {
                        if view.camera_mode {
                            view.target_scroll.y += 50.0 / view.target_zoom;
                        }
                    }
                    VirtualKeyCode::W => {
                        view.wireframe = !view.wireframe;
                    }
                    VirtualKeyCode::A => {
                        view.target_stroke_width /= 0.8;
                    }
                    VirtualKeyCode::Z => {
                        view.target_stroke_width *= 0.8;
                    }
                    VirtualKeyCode::R => {
                        view.reload = true;
                    }
                    VirtualKeyCode::Space => {
                        view.default_zoom = view.target_zoom;
                        view.default_scroll = view.default_scroll;
                        view.calibrate = true;
                    }
                    VirtualKeyCode::LControl => {
                        view.camera_mode = !view.camera_mode;
                        if !view.camera_mode {
                            view.target_scroll = view.default_scroll;
                            view.target_zoom = view.default_zoom;
                            view.wireframe = false;
                        }
                    }
                    _key => {}
                }
            }
            _ => {}
        }
        //println!(" -- [{}] zoom: {}, scroll: {:?}", view.current_slide, view.target_zoom, view.target_scroll);
    });

    view.zoom += (view.target_zoom - view.zoom) / 3.0;
    view.scroll = view.scroll + (view.target_scroll - view.scroll) / 3.0;
    view.stroke_width = view.stroke_width +
        (view.target_stroke_width - view.stroke_width) / 5.0;

    return status;
}


pub struct Context {
    fill_tess: FillTessellator,
    stroke_tess: StrokeTessellator,
}

pub struct RenderScene {
    primitives: Vec<Primitive>,
    transforms: Vec<Transform>,
    geometry: VertexBuffers<GpuVertex>,
    color_1: [f32; 4],
    color_2: [f32; 4],
    blueprint: f32,
}

//#[derive(Deserialize)]
pub struct Scene {
    items: Vec<Element>,
}

//#[derive(Deserialize)]
pub struct Element {
    path: PathData,
    fill: Option<Fill>,
    stroke: Option<Stroke>,
    transform: Transform2D<f32>,
}

//#[derive(Deserialize)]
pub struct Fill {
    z_index: i32,
    color: [f32; 4],
    tolerance: f32,
}

//#[derive(Deserialize)]
pub struct Stroke {
    z_index: i32,
    color: [f32; 4],
    options: StrokeOptions,
}

//#[derive(Deserialize)]
pub enum PathData {
    String(String),
    Path(Path),
}

const DEFAULT_TOLERANCE: f32 = 0.1;

impl Scene {
    fn load_svg(src_path: &str) -> Self {
        let mut scene = Scene {
            items: Vec::new(),
        };

        let svg_options = resvg::Options::default();
        let rtree = resvg::parse_rtree_from_file(src_path, &svg_options).unwrap();

        let mut z_index = 1;

        for node in rtree.root().descendants() {
            if let resvg::tree::NodeKind::Path(ref p) = *node.value() {

                let t = node.value().transform();

                let transform = Transform2D::row_major(
                    t.a as f32, t.b as f32,
                    t.c as f32, t.d as f32,
                    t.e as f32, t.f as f32,
                );

                let path = PathData::Path(convert_path(p));
                let mut item = Element {
                    path,
                    fill: None,
                    stroke: None,
                    transform,
                };

                if let Some(fill) = p.fill {
                    item.fill = Some(Fill {
                        color: convert_color(&fill.paint, fill.opacity),
                        tolerance: DEFAULT_TOLERANCE,
                        z_index,
                    });

                    z_index += 1;
                }

                if let Some(ref stroke) = p.stroke {
                    let linecap = match stroke.linecap {
                        tree::LineCap::Butt => tessellation::LineCap::Butt,
                        tree::LineCap::Square => tessellation::LineCap::Square,
                        tree::LineCap::Round => tessellation::LineCap::Round,
                    };

                    let linejoin = match stroke.linejoin {
                        tree::LineJoin::Miter => tessellation::LineJoin::Miter,
                        tree::LineJoin::Bevel => tessellation::LineJoin::Bevel,
                        tree::LineJoin::Round => tessellation::LineJoin::Round,
                    };

                    item.stroke = Some(Stroke {
                        color: convert_color(&stroke.paint, stroke.opacity),
                        options: StrokeOptions::tolerance(DEFAULT_TOLERANCE)
                            .with_line_width(stroke.width as f32)
                            .with_line_cap(linecap)
                            .with_line_join(linejoin),
                        z_index,
                    });

                    z_index += 1;
                }

                scene.items.push(item);
            }
        }

        scene
    }

    fn build(&self, ctx: &mut Context, color_1: [f32; 4], color_2: [f32; 4], blueprint: f32) -> RenderScene {
        let mut primitives = Vec::with_capacity(shaders::PRIM_BUFFER_LEN);
        let mut transforms = Vec::with_capacity(shaders::PRIM_BUFFER_LEN);
        let mut geometry = VertexBuffers::new();

        for item in self.items.iter().rev() {
            let path = match item.path {
                PathData::Path(ref path) => path.clone(),
                PathData::String(ref path_str) => build_path(Path::builder().with_svg(), &path_str).unwrap(),
            };

            let transform_idx = transforms.len() as i32;

            transforms.push(Transform {
                data0: [
                    item.transform.m11, item.transform.m12,
                    item.transform.m21, item.transform.m22,
                ],
                data1: [
                    item.transform.m31, item.transform.m32,
                    0.0, 0.0,
                ],
            });

            if let Some(ref fill) = item.fill {
                primitives.push(
                    Primitive {
                        color: fill.color,
                        z_index: fill.z_index,
                        transform: transform_idx,
                        user_data: [0.0, 0.0],
                    },
                );

                ctx.fill_tess.tessellate_path(
                    path.path_iter(),
                    &FillOptions::tolerance(fill.tolerance),
                    &mut BuffersBuilder::new(
                        &mut geometry,
                        WithId(primitives.len() as i32 - 1),
                    ),
                ).expect("Error during tesselation!");
            };

            if let Some(ref stroke) = item.stroke {
                primitives.push(
                    Primitive {
                        color: stroke.color,
                        z_index: stroke.z_index,
                        transform: transform_idx,
                        user_data: [0.0, 0.0],
                    },
                );

                ctx.stroke_tess.tessellate_path(
                    path.path_iter(),
                    &stroke.options,
                    &mut BuffersBuilder::new(
                        &mut geometry,
                        WithId(primitives.len() as i32 - 1),
                    ),
                );
            }
        }

        RenderScene {
            primitives,
            transforms,
            geometry,
            color_1,
            color_2,
            blueprint
        }
    }
}

struct GpuScene {
    primitives: PrimitiveBuffer,
    transforms: TransformBuffer,
    geometry: GeomBuffer,
    range: GeomRange,
}

impl GpuScene {
    fn new(scene: &RenderScene, factory: &mut Factory, cmd_queue: &mut CommandQueue) -> Self {
        assert!(scene.primitives.len() <= shaders::PRIM_BUFFER_LEN);

        let (geometry, range) = factory.create_vertex_buffer_with_slice(
            &scene.geometry.vertices[..],
            &scene.geometry.indices[..]
        );

        let primitives = factory.create_constant_buffer(shaders::PRIM_BUFFER_LEN);
        let transforms = factory.create_constant_buffer(shaders::PRIM_BUFFER_LEN);

        cmd_queue.update_buffer(
            &primitives,
            &scene.primitives[..],
            0
        ).unwrap();

        cmd_queue.update_buffer(
            &transforms,
            &scene.transforms[..],
            0
        ).unwrap();

        GpuScene {
            primitives,
            geometry,
            transforms,
            range,
        }
    }
}

type CommandQueue = gfx::Encoder<gfx_device_gl::Resources, gfx_device_gl::CommandBuffer>;
type PrimitiveBuffer = gfx::handle::Buffer<gfx_device_gl::Resources, Primitive>;
type TransformBuffer = gfx::handle::Buffer<gfx_device_gl::Resources, Transform>;
type GeomBuffer = gfx::handle::Buffer<gfx_device_gl::Resources, GpuVertex>;
type GeomRange = gfx::Slice<gfx_device_gl::Resources>;
type Factory = gfx_device_gl::Factory;

fn load_slides() -> Vec<RenderScene> {
    use std::io::BufReader;
    use std::io::BufRead;
    use std::fs::File;

    let mut slides = Vec::new();
    let mut ctx = Context {
        fill_tess: FillTessellator::new(),
        stroke_tess: StrokeTessellator::new(),
    };

    let list_file = File::open("slides.list").unwrap();
    let reader = BufReader::new(&list_file);
    for line in reader.lines() {
        let mut line = line.unwrap();
        let mut line = line.split_whitespace();
        let name = line.next().unwrap();
        let scene = Scene::load_svg(&format!("slides/{}", name));
        let color_1: [f32; 4] = [
            line.next().unwrap().parse().unwrap(),
            line.next().unwrap().parse().unwrap(),
            line.next().unwrap().parse().unwrap(),
            1.0,
        ];

        let color_2: [f32; 4] = [
            line.next().unwrap().parse().unwrap(),
            line.next().unwrap().parse().unwrap(),
            line.next().unwrap().parse().unwrap(),
            1.0,
        ];

        let blueprint: f32 = line.next().unwrap().parse().unwrap();

        println!("{:?} - {:?}/{:?}", name, color_1, color_2);

        slides.push(scene.build(&mut ctx, color_1, color_2, blueprint));
    }

    slides
}
