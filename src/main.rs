#[macro_use] extern crate log;
extern crate env_logger;
extern crate imageproc;
extern crate conv;
extern crate hyper;
extern crate url; 
extern crate gif;

use std::fs::File;

use image::gif::Decoder;
use gif::{ Encoder, Repeat };

use image::*;
use imageproc::pixelops::weighted_sum;

use imageproc::definitions::Clamp; 

use rusttype::{Font, point, FontCollection, Scale, PositionedGlyph};

use conv::ValueInto;

use hyper::{Body, Request, Response, Server};
use hyper::rt::Future;
use hyper::service::service_fn_ok;

const FONT_SCALE: Scale = Scale { x: 18.0, y: 18.0 }; 
const CAPTION_PADDING_HORIZ: u32 = 16;
const CAPTION_PADDING_BOTTOM: u32 = 20;

pub fn draw_text_centered_mut<'a, I>(
    image: &'a mut I,
    color: I::Pixel,
    font: &'a Font<'a>,
    text: &'a str,
    y: u32,
) where
    I: GenericImage,
    <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
{
    let scale = Scale::uniform(18.0);

    let v_metrics = font.v_metrics(scale);
    let offset = point(0.0, v_metrics.ascent);

    let image_width = image.width() as i32;
    let image_height = image.height() as i32;

    let glyphs: Vec<PositionedGlyph<'_>> = 
        font.layout(text, scale, offset).collect();

    //width of the entire string
    let mut str_width_px: i32 = 0; 
    // for g in glyphs.clone() { 
    //     if let Some(bb) = g.pixel_bounding_box() { 
    //         str_width_px = str_width_px + bb.max.x;  
    //     }
    // }
    if let Some(g) = glyphs.clone().last() { 
        if let Some(bb) = g.pixel_bounding_box() { 
            str_width_px = bb.max.x
        }
    }

    // centered x pos
    let x = (image_width / 2) - (str_width_px / 2);

    for g in glyphs {
        if let Some(bb) = g.pixel_bounding_box() {
            g.draw(|gx, gy, gv| {
                let gx = gx as i32 + bb.min.x;
                let gy = gy as i32 + bb.min.y;

                let image_x = gx + x as i32;
                let image_y = gy + y as i32;

                if image_x >= 0 && image_x < image_width && image_y >= 0 && image_y < image_height {
                    let pixel = image.get_pixel(image_x as u32, image_y as u32);
                    let weighted_color = weighted_sum(pixel, color, 1.0 - gv, gv);
                    image.put_pixel(image_x as u32, image_y as u32, weighted_color);
                }
            })
        }
    }
}

fn split_into_lines(caption: &str, font: &Font, font_size: Scale, max_line_px: f32)-> Vec<String> { 
    let words = caption.split_whitespace();

    let mut lines = Vec::new(); 
    let mut cur_line = String::from(""); 
    let mut cur_line_width: f32 = 0.0; 

    let space_width = font.glyph(' ').scaled(font_size).h_metrics().advance_width;

    for w in words {
        let mut word_width_px = 0.0;
        for g in font.glyphs_for(w.chars()) { 
            let h_metrics = g.scaled(font_size).h_metrics();
            word_width_px = word_width_px + h_metrics.advance_width; 
        }

        if !cur_line.is_empty() { 
            cur_line_width += space_width;
        }

        cur_line_width += word_width_px; 

        // TODO edge case: is the word width greater than the line width 
        // and have we just started a new line? smash it in anyway. 
    
        // can the word fit on this line? 
        if cur_line_width > max_line_px { 
            //no, start a new line and put the word there

            //...unless we just now created a new line, then put it on anyway. 
            if cur_line.is_empty()  { 
                cur_line.push_str(w); 
                lines.push(cur_line);
                cur_line = String::from(""); 
                cur_line_width = 0.0;
            } else { 
                //start a new line and put the word that didn't fit on it
                lines.push(cur_line);
                cur_line = String::from(""); 
                cur_line.push_str(w); 
                cur_line_width = word_width_px; 
            }
        } else { 
            if !cur_line.is_empty() { 
                cur_line.push(' '); 
            }

            //yes, append it to the current line 
            cur_line.push_str(w); 
        }
    }

    if !cur_line.is_empty() { 
        lines.push(cur_line);
    }
    
    lines
}

fn render_bottom_caption_mut<'a, I>(
    image: &'a mut I,
    color: I::Pixel,
    font: &'a Font<'a>,
    caption: &'a str,
) where
    I: GenericImage,
    <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>, 
{ 
    let (im_width, im_height) = image.dimensions();
    let max_line_px = (im_width - (CAPTION_PADDING_HORIZ * 2)) as f32; 
    let lines = split_into_lines(caption, font, FONT_SCALE, max_line_px); 

    let v_metrics = font.v_metrics(FONT_SCALE);

    let line_height: u32 = (v_metrics.ascent + (v_metrics.descent * -1.0) + v_metrics.line_gap) as u32;
    let mut line_y = im_height - CAPTION_PADDING_BOTTOM - (line_height * (lines.len() as u32));
    for line in lines { 
        draw_text_centered_mut(image, color, font, &line, line_y);
        line_y += line_height;
    }
}

fn load_binderslap_gif() -> Vec<Frame> {  
    let file_in = File::open("binderslap_opt.gif").unwrap();
    let decoder = Decoder::new(file_in).unwrap();
    let frames = decoder.into_frames();
    frames.collect_frames().expect("error decoding gif")
}

fn create_binderslap_gif(input_frames: Vec<Frame>, font: &Font<'static>, caption: String) -> Vec<image::gif::Frame<'static>> { 
    let mut out_frames = Vec::new();

    let frame_start = 12; 
    let frame_end = 36; 
    let num_frames = input_frames.len();
    let mut cur_frame = 1;

    //TODO investigate a way to stream frames to HTTP response instead
    //of buffering entire GIF
    for f in input_frames { 
        let delay = f.delay().to_integer();
        let mut buf = f.into_buffer(); 
        let (fw,fh) = buf.dimensions();

        if cur_frame > frame_start && cur_frame < frame_end { 
            render_bottom_caption_mut(
                &mut buf, 
                Rgba([255u8, 255u8, 255u8, 255u8]), 
                &font, 
                &caption,
            );
        }

        let mut data = buf.into_raw();

        // potential ways to make this faster: 
        // 1. use RGB instead fo RGBA 
        // 2. parallelize the RGB -> frame conversion
        // 3. do all operations on indexed pixel data instead of rgb
        let mut gif_frame = image::gif::Frame::from_rgba_speed(fw as u16, fh as u16, data.as_mut_slice(), 30); 

        gif_frame.delay = 6;//delay; //FIXME reuse delay from original GIF
        out_frames.push(gif_frame);
        cur_frame = cur_frame + 1;
    }

    out_frames
}

fn main() {
    env_logger::init();

    let input_frames = load_binderslap_gif();
    let num_frames = input_frames.len(); 
    let font = Vec::from(include_bytes!("../DejaVuSans.ttf") as &[u8]);
    let font = FontCollection::from_bytes(font).unwrap().into_font().unwrap();

    let port = match std::env::var("PORT") { 
        Ok(v) => v.to_string(),
        Err(_) => String::from("9000"), //default port
    }; 

    // This is our socket address...
    let addr = [ "0.0.0.0", &port ].join(":").parse().unwrap();

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let new_svc = move || {
        let input_frames = input_frames.clone(); 
        let font = font.clone();
        let image_service = move |req: Request<Body>| {
            use url::form_urlencoded; 
            use std::collections::HashMap;

            if req.uri().path() != "/image" { 
                Response::builder()
                    .status(404)
                    .body(Body::from("go away"))
                    .unwrap()
            } else { 
                let caption = match req.uri().query() { 
                    Some(query) => {
                        let mut param_map = form_urlencoded::parse(query.as_ref()).into_owned().collect::<HashMap<String, String>>();
                        match param_map.remove("t") { 
                            Some(caption) => caption, 
                            None => String::from("Hello")
                        }
                    },
                    None => String::from("hello"),
                };
                
                let out_frames = create_binderslap_gif(input_frames.clone(), &font.clone(), caption); 
                let mut buf_out = Vec::new();

                {
                    use crate::gif::SetParameter;
                    let b = &mut buf_out;
                    let mut gif_encoder = Encoder::new(b, 480, 240, &[]).unwrap();
                    let mut cur_frame = 1;

                    gif_encoder.set(Repeat::Infinite).unwrap();

                    for f in out_frames { 
                        println!("encoding frame {} of {}", cur_frame, num_frames);
                        gif_encoder.write_frame(&f).unwrap();
                        cur_frame = cur_frame + 1;
                    }
                }

                Response::builder()
                    .header(hyper::header::CONTENT_TYPE, "image/gif")
                    .body(Body::from(buf_out))
                    .unwrap()
            }
        };

        // service_fn_ok converts our function into a `Service`
        service_fn_ok(image_service)
    };

    let server = Server::bind(&addr)
        .serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("starting server"); 

    // Run this server for... forever!
    hyper::rt::run(server);
}
