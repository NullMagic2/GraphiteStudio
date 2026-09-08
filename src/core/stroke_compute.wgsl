struct Pixel { v:array<f32,16>, }
struct Dab { v:array<f32,64>, }
struct Metadata { region:vec4<u32>, counts:vec4<u32>, }
@group(0) @binding(0) var<storage,read_write> pixels:array<Pixel>;
@group(0) @binding(1) var<storage,read> dabs:array<Dab>;
@group(0) @binding(2) var<storage,read> mask:array<u32>;
@group(0) @binding(3) var<storage,read> profile:array<f32>;
@group(0) @binding(4) var<storage,read_write> wear:array<atomic<u32>>;
@group(0) @binding(5) var<uniform> params:Metadata;
@group(0) @binding(6) var<storage,read> coordinates:array<u32>;
fn ease(v:f32)->f32 {let t=clamp(v,0.,1.);return t*t*(3.-2.*t);}
// Near zero, exp() accuracy varies by driver. Preserve the CPU's f32
// deposition threshold: exp(-x) is rounded on the [0.5, 1] float grid before
// it is subtracted from one. This also avoids tiny transfer differences
// triggering an extra full pressure-packing step near saturation.
fn deposition_integral(x:f32)->f32 {
    if x<0.125 {
        let y=x*(1.-x*(0.5-x*(0.1666666667-x*(0.04166666667-x*(0.008333333333-x*0.001388888889)))));
        return floor(y*16777216.+0.5)/16777216.;
    }
    return 1.-exp(-x);
}
fn sample_mask(uv:vec2<f32>)->f32 {
    if any(uv<vec2(0.)) || any(uv>vec2(1.)){return 0.;}
    let size=params.counts.zw;let p=uv*vec2<f32>(size-vec2(1u));let a=vec2<u32>(p);let b=min(a+vec2(1u),size-vec2(1u));let f=fract(p);
    let v=vec4(f32(mask[a.y*size.x+a.x]),f32(mask[a.y*size.x+b.x]),f32(mask[b.y*size.x+a.x]),f32(mask[b.y*size.x+b.x]))/255.;
    return mix(mix(v.x,v.y,f.x),mix(v.z,v.w,f.x),f.y);
}
fn rotate(p:vec2<f32>,s:f32,c:f32)->vec2<f32>{return vec2(p.x*c+p.y*s,-p.x*s+p.y*c);}
fn sample_profile(p:vec2<f32>)->f32 {
    let n=params.counts.y;let q=clamp((p*0.5+vec2(0.5))*f32(n)-vec2(0.5),vec2(0.),vec2(f32(n-1u)));
    let a=vec2<u32>(q);let b=min(a+vec2(1u),vec2(n-1u));let f=fract(q);
    return mix(mix(profile[a.y*n+a.x],profile[a.y*n+b.x],f.x),mix(profile[b.y*n+a.x],profile[b.y*n+b.x],f.x),f.y);
}
fn add_wear(i:u32,value:f32){
    var old=atomicLoad(&wear[i]);
    loop {let result=atomicCompareExchangeWeak(&wear[i],old,bitcast<u32>(bitcast<f32>(old)+value));if result.exchanged {break;}old=result.old_value;}
}
fn contact_wear(p:vec2<f32>,load:f32,d:Dab){
    let q=vec2(p.x*d.v[7]-p.y*d.v[6],p.x*d.v[6]+p.y*d.v[7]);
    if dot(q,q)>1.08{return;}
    let amount=pow(load,1.08)*d.v[32]*d.v[20]*(0.42+0.78*(1.-d.v[24]))*(1.12-0.64*d.v[34])*0.00000035;
    if amount<=1e-10{return;}
    let n=params.counts.y;let xy=clamp((q*0.5+vec2(0.5))*f32(n)-vec2(0.5),vec2(0.),vec2(f32(n-1u)));
    let a=vec2<u32>(xy);let b=min(a+vec2(1u),vec2(n-1u));let f=fract(xy);
    add_wear(a.y*n+a.x,amount*(1.-f.x)*(1.-f.y));add_wear(a.y*n+b.x,amount*f.x*(1.-f.y));
    add_wear(b.y*n+a.x,amount*(1.-f.x)*f.y);add_wear(b.y*n+b.x,amount*f.x*f.y);
}
fn profile_coordinate(p:vec2<f32>,d:Dab)->vec2<f32>{return vec2(p.x*d.v[7]-p.y*d.v[6],p.x*d.v[6]+p.y*d.v[7]);}
fn fresh_height(p:vec2<f32>)->f32{
    let r=length(p);if r>1.{return 0.;}
    let angle=atan2(p.y,p.x);let facet=0.018*cos(6.*angle)+0.010*cos(11.*angle+0.7);
    return clamp(clamp(1.-pow(r,1.34),0.,1.)*(0.985+facet),0.,1.);
}
fn recession(p:vec2<f32>)->f32{if dot(p,p)>1.{return 0.;}return clamp(fresh_height(p)-sample_profile(p),0.,1.);}
fn grain_hash(p:vec2<i32>)->f32{
    var h=(bitcast<u32>(p.x)*0x9e3779b9u)^(bitcast<u32>(p.y)*0x85ebca6bu)^0x491e3197u;
    h^=h>>16u;h*=0x7feb352du;h^=h>>15u;h*=0x846ca68bu;h^=h>>16u;return f32(h)/4294967295.;
}
fn contact_grain(p:vec2<f32>)->f32{
    let a=vec2<i32>(floor(p));let t=vec2(ease(fract(p.x)),ease(fract(p.y)));
    return mix(mix(grain_hash(a),grain_hash(a+vec2(1,0)),t.x),mix(grain_hash(a+vec2(0,1)),grain_hash(a+vec2(1,1)),t.x),t.y);
}
// coverage, force, profile contact, and wear coordinates of the physical tip.
struct Contact {coverage:f32,weight:f32,face:f32,uv:vec2<f32>,}
fn native_contact(local:vec2<f32>,d:Dab,pixel:Pixel)->Contact {
    var result:Contact;
    let axial=clamp((d.v[44]-local.x)/max(d.v[44]-d.v[45],0.1),0.,1.);
    let width=mix(d.v[42],d.v[43],d.v[50]*ease(axial/0.72));
    let nx=clamp(abs(local.x-d.v[46])/max(d.v[47],0.12),0.,3.);
    let am=pow(nx,d.v[48]);let gx=d.v[48]*pow(nx,d.v[48]-1.)/max(d.v[47],0.12);
    let boundary=atan2(local.y/max(width,0.10),(local.x-d.v[46])/max(d.v[47],0.10));
    let angle=boundary-d.v[54];
    let facet=0.024*cos(5.*angle+0.35)+0.015*cos(8.*angle-0.90)+0.009*cos(11.*angle+1.70);
    let edge=clamp(1.+facet-0.085*recession(profile_coordinate(vec2(cos(boundary),sin(boundary))*0.86,d)),0.90,1.06);
    let ny=clamp(abs(local.y)/max(width*edge,0.10),0.,3.);
    let metric=am+pow(ny,d.v[49]);let gy=d.v[49]*pow(ny,d.v[49]-1.)/max(width*edge,0.10);
    let distance=(metric-1.)/max(length(vec2(gx,gy)),0.01);
    // This rotation follows the native contact asperities (+barrel rotation).
    let grain=contact_grain(vec2(local.x*d.v[7]+local.y*d.v[6],-local.x*d.v[6]+local.y*d.v[7])/d.v[51]);
    let sd=distance+(grain-0.5)*d.v[51]*2.4;
    if ease(0.5-sd)<=1e-5{return result;}
    let uv=clamp(vec2(local.y/max(width,0.10),(local.x-d.v[46])/max(d.v[47],0.10)),vec2(-1.15),vec2(1.15));
    let q=profile_coordinate(uv,d);let wear_depth=recession(q);
    let contact=select(ease((d.v[52]-wear_depth+0.020)/0.055),1.,wear_depth<=1e-6);
    if contact<=1e-5{return result;}
    let face=contact*(0.90+0.10*sqrt(fresh_height(q)));
    let weight=(0.78+0.22*pow(clamp(1.-metric,0.,1.),0.35))*(0.78+0.22*sqrt(face))*d.v[53]*(0.50+0.85*grain);
    let jitter=(floor(pixel.v[14]/256.)/255.-0.5)*d.v[57]*3.;
    let coverage=ease(0.5-(sd+jitter)/d.v[55])*contact;
    let relief=select(1.,0.18+0.82*sqrt(sample_mask(q*0.5+vec2(0.5))),d.v[56]>0.5);
    result.coverage=coverage;result.weight=weight*relief;result.face=face*relief;result.uv=uv;return result;
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid:vec3<u32>){
    let i=gid.x;let w=params.region.z;if i>=params.region.w{return;}
    var pixel=pixels[i];if pixel.v[15]==0.{return;}
    let offset=coordinates[i];
    let position=vec2<f32>(params.region.xy+vec2(offset%w,offset/w))+vec2(0.5);
    for(var k=0u;k<params.counts.x;k++){
        let d=dabs[k];let delta=position-vec2(d.v[0],d.v[1]);
        if any(position<vec2(d.v[58],d.v[59])) || any(position>=vec2(d.v[60],d.v[61])){continue;}
        let total=pixel.v[2]+pixel.v[3]+pixel.v[4];
        let capacity=0.38+1.55*clamp(1.-pixel.v[0],0.,1.)+0.24*pixel.v[12];
        if d.v[36]==1. {
            let distance=length(delta);let radius=d.v[39];let soft=select(0.22,0.45,d.v[38]>0.5);
            let coverage=ease((radius-distance+0.5)/max(radius*soft,1.));if coverage<=0.0001 || total<=0.{continue;}
            let weight=coverage*(0.82+0.18*clamp(1.-distance/max(radius,0.1),0.,1.));let strength=d.v[37];
            if strength>=1. {for(var c=2u;c<12u;c++){pixel.v[c]=0.;}continue;}
            let base=weight*pow(d.v[33],0.90)*strength*(0.4+0.6*pixel.v[0])*(0.84+0.16*pixel.v[13])*d.v[16];
            let loose_rate=select(1.70,1.15,d.v[38]>0.5);let compact_rate=select(0.24*(0.45+0.55*d.v[33]),0.045,d.v[38]>0.5);
            let loose=pixel.v[5]*(1.-exp(-base*loose_rate));let compact=pixel.v[6]*(1.-exp(-base*compact_rate));let removed=min(loose+compact,total);if removed<=1e-8{continue;}
            pixel.v[5]=max(pixel.v[5]-loose,0.);pixel.v[6]=max(pixel.v[6]-compact,0.);
            let ratio=clamp((total-removed)/total,0.,1.);for(var c=2u;c<12u;c++){if c!=5u && c!=6u{pixel.v[c]*=ratio;}}
            let work=weight*pow(d.v[33],1.45)*strength*d.v[16]*select(1.,0.03,d.v[38]>0.5);
            pixel.v[0]=clamp(pixel.v[0]-max(pixel.v[0]-0.5,0.)*work*0.0014,0.,1.);pixel.v[1]=clamp(pixel.v[1]+work*0.00075,0.,1.);continue;
        }
        let local=rotate(delta,d.v[4],d.v[5]);var coverage=0.;var face=1.;var weight=1.;var uv=vec2(0.);
        if d.v[36]==2. {
            let falloff=max(1.-dot(delta,delta)*d.v[41],0.);coverage=falloff*falloff*falloff;
        }else if d.v[36]==3. {
            let contact=native_contact(local,d,pixel);coverage=contact.coverage;face=contact.face;weight=contact.weight;uv=contact.uv;
        }else{
            let half_size=vec2(d.v[2],d.v[3]);if any(abs(local)>half_size+vec2(1.)){continue;}
            for(var t=0u;t<4u;t++){
                let offset=vec2(select(-0.25,0.25,(t&1u)!=0u),select(-0.25,0.25,(t&2u)!=0u));
                coverage+=sample_mask(rotate(delta+offset,d.v[4],d.v[5])/(2.*half_size)+vec2(0.5))*0.25;
            }
            if coverage<=1e-5{continue;}
            uv=local/half_size;var relief=0.;
            if dot(uv,uv)<=1.04 {relief=clamp(sample_profile(vec2(uv.x*d.v[7]-uv.y*d.v[6],uv.x*d.v[6]+uv.y*d.v[7]))/max(d.v[35],1e-5),0.,1.);}
            weight=(0.65+0.45*sqrt(coverage))*(0.7+0.3*relief);face=(0.65+0.35*relief)*sqrt(coverage);
        }
        if coverage<=1e-5{continue;}
        if d.v[36]==2. {
            var work=coverage*d.v[40]*(0.8+0.2*pixel.v[13]);
            if d.v[42]>0. {
                let density_patch=contact_grain(position*d.v[43]+vec2(17.3,31.7));
                let density=density_patch*0.8+(f32(u32(pixel.v[14])&255u)/255.)*0.2;
                work*=1.+d.v[42]*(density*2.-1.)*0.95;
            }
            let amount=max(capacity-total,0.)*(1.-exp(-work));if amount<=1e-8{continue;}
            pixel.v[2]+=amount*d.v[21];pixel.v[3]+=amount*d.v[22];pixel.v[4]+=amount*d.v[23];pixel.v[5]+=amount*0.85;pixel.v[6]+=amount*0.15;
            let tone=1.+(f32(u32(pixel.v[14])&255u)/255.-0.5)*d.v[31]*0.3;
            for(var c=0u;c<3u;c++){pixel.v[9u+c]+=amount*clamp(d.v[28u+c]*tone,0.,1.);}continue;
        }
        let normal=clamp(weight*d.v[8],0.,1.75);if normal<=1e-6{continue;}
        let geometric=ease(0.5+(clamp(pixel.v[0],0.,1.)-d.v[9])/d.v[10]);let fraction=(d.v[11]+(1.-d.v[11])*geometric)*clamp(face,0.,1.);
        let capture=clamp(fraction*(0.70+0.5*pixel.v[13])*(1.+0.08*d.v[12]),0.,1.25);if fraction<=0.002 || capture<=0.002{continue;}
        let work=d.v[13]*pow(normal,0.82)*d.v[14]*clamp(capture*coverage,0.,1.)*(0.70+0.38*pixel.v[12])*d.v[27]*d.v[15]*d.v[16]*0.72;
        let amount=max(capacity-total,0.)*deposition_integral(work/capacity);if amount<=2e-6{continue;}
        pixel.v[2]+=amount*d.v[21];pixel.v[3]+=amount*d.v[22];pixel.v[4]+=amount*d.v[23];
        let grain=(f32(u32(pixel.v[14])&255u)/255.-0.5)*2.;let tone=clamp(1.+grain*select(0.80,1.25,grain<0.)*clamp(d.v[31],0.,0.35),0.55,1.30);
        for(var c=0u;c<3u;c++){pixel.v[9u+c]+=amount*clamp(d.v[28u+c]*tone,0.,1.);}
        let initial=amount*d.v[17];pixel.v[5]+=amount-initial;pixel.v[6]+=initial;
        let packing=normal*d.v[16]/0.17*(0.003+0.0065*d.v[24]+0.0025*d.v[25]);let packed=pixel.v[5]*clamp(packing,0.,0.11);pixel.v[5]-=packed;pixel.v[6]+=packed;
        pixel.v[7]+=amount*d.v[18];pixel.v[8]+=amount*d.v[19];
        let peak=max(pixel.v[0]-0.5,0.);if peak>0.{let smoothing=pow(normal,1.65)*d.v[16]/0.17*(0.0007+0.0032*d.v[24])*(1.25-0.55*d.v[26]);pixel.v[0]=clamp(pixel.v[0]-peak*smoothing,0.,1.);}
        pixel.v[1]=clamp(pixel.v[1]+pow(normal,1.35)*d.v[16]/0.17*(0.00018+0.00062*d.v[24])*(1.20-0.65*d.v[26]),0.,1.);
        contact_wear(uv,normal,d);
    }
    pixels[i]=pixel;
}
