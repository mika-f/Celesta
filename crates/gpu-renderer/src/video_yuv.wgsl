struct Params {
    shape: vec4<u32>,
    color: vec4<u32>,
}
@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var chroma: texture_2d<f32>;
@group(0) @binding(2) var chroma_v: texture_2d<f32>;
@group(0) @binding(3) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> params: Params;

fn cubic(d: f32) -> f32 {
    if d < 1.0 { return 1.4*d*d*d - 2.4*d*d + 1.0; }
    if d < 2.0 { return -0.6*d*d*d + 3.0*d*d - 4.8*d + 2.4; }
    return 0.0;
}

fn uv_at(x: u32, y: u32) -> vec2<f32> {
    if params.shape.z == 0u {
        let p=vec2<i32>(i32(x/2u),i32(y/2u));
        return round(vec2<f32>(textureLoad(chroma,p,0).r,textureLoad(chroma_v,p,0).r)*255.0);
    }
    let h=i32((params.shape.y+1u)/2u);
    let taps=clamp(h-2,1,5);
    let position=f32(y)*0.5-0.25;
    let first=i32(position-(f32(taps)-2.0)*0.5);
    var sum=0.0;
    for(var i=0;i<taps;i++) {sum+=cubic(abs(position-f32(first+i)));}
    var uv=vec2<f32>(0.0);
    for(var i=0;i<taps;i++) {
        let row=first+i;
        let weight=cubic(abs(position-f32(row)))/sum;
        let p=vec2<i32>(i32(x/2u),clamp(row,0,h-1));
        uv+=weight*round(textureLoad(chroma,p,0).rg*255.0);
    }
    return uv;
}

fn write_rgb(p: vec2<u32>, chroma_value: vec2<f32>) {
    if p.x>=params.shape.x || p.y>=params.shape.y {return;}
    var kr=0.299;
    var kb=0.114;
    if params.color.x==1u {kr=0.2126;kb=0.0722;}
    let uv=chroma_value-vec2<f32>(128.0);
    var y=round(textureLoad(luma,vec2<i32>(p),0).r*255.0);
    var scale=1.0;
    if params.color.y==0u {y=(y-16.0)*255.0/219.0;scale=255.0/224.0;}
    let u=uv.x*scale;
    let v=uv.y*scale;
    let rgb=vec3<f32>(y+2.0*(1.0-kr)*v,
        y-2.0*kb*(1.0-kb)/(1.0-kr-kb)*u-2.0*kr*(1.0-kr)/(1.0-kr-kb)*v,
        y+2.0*(1.0-kb)*u);
    textureStore(output,vec2<i32>(p),vec4<f32>(clamp(floor(rgb),vec3<f32>(0.0),vec3<f32>(255.0))/255.0,1.0));
}

@compute @workgroup_size(8,8)
fn convert(@builtin(global_invocation_id) id: vec3<u32>) {
    if params.shape.z==1u {
        if id.x>=params.shape.x || id.y>=params.shape.y {return;}
        write_rgb(id.xy,uv_at(id.x,id.y));
    } else {
        let p=id.xy*2u;
        if p.x>=params.shape.x || p.y>=params.shape.y {return;}
        let uv=uv_at(p.x,p.y);
        write_rgb(p,uv);
        write_rgb(p+vec2<u32>(1u,0u),uv);
        write_rgb(p+vec2<u32>(0u,1u),uv);
        write_rgb(p+vec2<u32>(1u,1u),uv);
    }
}
