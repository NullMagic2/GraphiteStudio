import sys,struct,json
from pathlib import Path
from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import PixelLayer
from psd_tools.constants import Tag
from psd_tools.psd.vector import VectorMaskSetting,ClosedPath,OpenPath,ClosedKnotUnlinked,OpenKnotUnlinked,Path as PSDPath,InitialFillRule,PathFillRule
from psd_tools.psd.descriptor import DescriptorBlock,Descriptor,Double,UnitFloat,Enumerated,Bool,List
from psd_tools.psd.tagged_blocks import TaggedBlock
from psd_tools.psd.image_resources import ImageResource
from psd_tools.terminology import Unit
root=Path(__file__).resolve().parent
W,H=64,48
p=PSDImage.new('RGBA',(W,H))
PixelLayer.frompil(Image.new('RGBA',(W,H),(220,220,220,255)),p,'Raster base')
def knot(x,y,before=None,after=None,cls=ClosedKnotUnlinked):
 def xy(pt):return (pt[1]/H,pt[0]/W)
 return cls(preceding=xy(before or (x,y)),anchor=xy((x,y)),leaving=xy(after or (x,y)))
def solid(rgb):return DescriptorBlock(items={b'Clr ':Descriptor(classID=b'RGBC',items={k:Double(v) for k,v in zip((b'Rd  ',b'Grn ',b'Bl  '),rgb)})})
def tag(layer,key,data):layer.tagged_blocks[key]=TaggedBlock(key=key,data=data)
shape=PixelLayer.frompil(Image.new('RGBA',(W,H),(255,0,255,255)),p,'Bezier shape with hole')
outer=ClosedPath(items=[knot(8,8,after=(24,0)),knot(48,8,before=(32,0)),knot(48,40),knot(8,40)],operation=1)
hole=ClosedPath(items=[knot(20,16),knot(36,16),knot(36,32),knot(20,32)],operation=2)
path=PSDPath(items=[PathFillRule(),InitialFillRule(0),outer,hole])
tag(shape,Tag.VECTOR_MASK_SETTING1,VectorMaskSetting(path=path))
tag(shape,Tag.SOLID_COLOR_SHEET_SETTING,solid((240,70,30)))
shape.opacity=180
stroke=PixelLayer.frompil(Image.new('RGBA',(W,H),(255,0,255,255)),p,'Blue curve')
curve=OpenPath(items=[knot(4,24,after=(15,2),cls=OpenKnotUnlinked),knot(60,24,before=(49,46),cls=OpenKnotUnlinked)],operation=1)
curvepath=PSDPath(items=[PathFillRule(),InitialFillRule(0),curve])
tag(stroke,Tag.VECTOR_MASK_SETTING2,VectorMaskSetting(path=curvepath))
style=DescriptorBlock(classID=b'strokeStyle',items={b'fillEnabled':Bool(False),b'strokeEnabled':Bool(True),b'strokeStyleLineWidth':UnitFloat(3,Unit.Pixels),b'strokeStyleLineCapType':Enumerated(b'strokeStyleLineCapType',b'strokeStyleRoundCap'),b'strokeStyleLineJoinType':Enumerated(b'strokeStyleLineJoinType',b'strokeStyleRoundJoin'),b'strokeStyleLineAlignment':Enumerated(b'strokeStyleLineAlignment',b'strokeStyleAlignCenter'),b'strokeStyleOpacity':UnitFloat(100,Unit.Percent),b'strokeStyleContent':Descriptor(items=solid((30,80,230)).items())})
tag(stroke,Tag.VECTOR_STROKE_DATA,style)
p._record.image_resources[2000]=ImageResource(key=2000,name='Saved curve',data=curvepath)
with (root/'shape-layers.psd').open('wb') as f:p._record.write(f)
q=PSDImage.open(root/'shape-layers.psd')
manifest=[]
for l in q:
 if not l.has_vector_mask():continue
 manifest.append(dict(name=l.name,paths=[dict(closed=s.is_closed(),operation=s.operation,knots=[dict(before=[k.preceding[1]*W,k.preceding[0]*H],anchor=[k.anchor[1]*W,k.anchor[0]*H],after=[k.leaving[1]*W,k.leaving[0]*H]) for k in s]) for s in l.vector_mask.paths]))
(root/'shape-manifest.json').write_text(json.dumps(manifest),encoding='utf-8')
# Must report unsupported features instead of silently rasterizing this layer.
style[b'strokeStyleLineAlignment']=Enumerated(b'strokeStyleLineAlignment',b'strokeStyleAlignInside')
with (root/'unsupported-vector-style.psd').open('wb') as f:p._record.write(f)
print('Created independent cubic fill, hole, stroke, saved-path and unsupported-style PSDs.')

p=PSDImage.new('RGBA',(W,H))
fill_layer=PixelLayer.frompil(Image.new('RGBA',(W,H),(255,0,255,255)),p,'Solid fill')
tag(fill_layer,Tag.SOLID_COLOR_SHEET_SETTING,solid((20,100,200)))
tag(fill_layer,Tag.BLEND_FILL_OPACITY,b'\x80')
with (root/'solid-fill.psd').open('wb') as f:p._record.write(f)
