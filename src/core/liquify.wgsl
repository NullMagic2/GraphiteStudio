struct Params { a:vec4<f32>, b:vec4<f32>, c:vec4<f32>, d:vec4<f32> }
@group(0) @binding(0) var<storage,read> points:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> result:array<vec2<f32>>;
@group(0) @binding(2) var<uniform> params:Params;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    let i=id.x;if i>=u32(params.c.w) {return;}
    let p=points[i];let q=p.xy;let center=params.a.xy;let delta=params.a.zw;
    let direction=params.b.xy;let radius=params.b.z;let strength=params.b.w;
    let dose=params.c.x;let chaos=params.c.y;let mode=u32(params.c.z);
    let d=q-center;let r=length(d)/radius;let edge=max(0.,1.-r*r);let f=edge*edge;let k=strength*f*dose;
    if mode==7u {
        var next=p.zw*(1.-clamp(k*0.5,0.,1.));if length(next)<0.01 {next=vec2(0.);}
        result[i]=q+next;return;
    }
    let normal=vec2(-direction.y,direction.x);
    let wave=sin(d.x/radius*19.+sin(d.y/radius*13.)*2.);
    var src=q;
    switch mode {
        case 0u: {src=q-delta*strength*f;}
        case 1u,2u: {
            var angle=k*0.24*(1.+chaos*wave*1.6);if mode==2u {angle=-angle;}
            let sn=sin(angle);let cs=cos(angle);src=center+vec2(d.x*cs-d.y*sn,d.x*sn+d.y*cs);
        }
        case 3u: {src=center+d*exp(k*0.22);}
        case 4u: {src=center+d*exp(-k*0.22);}
        case 5u: {
            var angle=0.;if dot(d,d)>0. {angle=atan2(d.y,d.x);}
            let facets=abs(sin(angle*9.+r*8.));src=center+d*exp(-k*(0.07+0.5*facets)*(1.+chaos*2.));
        }
        case 6u: {src=q+normal*dot(d,normal)*(exp(k*0.4)-1.);}
        default: {}
    }
    if chaos>0. && mode!=1u && mode!=2u && mode!=5u {
        var amount=dose;if mode==0u {amount=length(delta)/radius;}
        src+=vec2(-d.y,d.x)*wave*chaos*strength*f*amount*0.22;
    }
    result[i]=src;
}
