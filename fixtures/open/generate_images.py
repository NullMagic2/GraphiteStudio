import sys, json, struct
from pathlib import Path
from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import PixelLayer, Group
from psd_tools.constants import Compression, BlendMode, Tag
from psd_tools.psd.layer_and_mask import LayerInfo, LayerInfoBlock

root=Path(__file__).resolve().parent
root.mkdir(parents=True,exist_ok=True)
manifest=[]
for depth in (8,16,32):
    for compression in Compression:
        psd=PSDImage.new('RGBA',(7,5),depth=8)
        base=PixelLayer.frompil(Image.new('RGBA',(7,5),(200,140,60,255)),psd,'Base café',compression=compression)
        over=Image.new('RGBA',(4,3))
        over.putdata([(x*60,y*90,210,60+x*40) for y in range(3) for x in range(4)])
        top=PixelLayer.frompil(over,psd,'Offset α',left=-1,top=1,compression=compression)
        top.opacity=137
        top.blend_mode=BlendMode.MULTIPLY
        hidden=PixelLayer.frompil(Image.new('RGBA',(2,2),(30,50,220,255)),psd,'Hidden',left=4,top=2,compression=compression)
        hidden.visible=False
        for layer in psd:
            layer._record.name=layer.name.encode('macroman','replace').decode('macroman')
        if depth!=8:
            for layer in psd:
                for channel in layer._channels:
                    raw=channel.get_data(layer.width,layer.height,8)
                    promoted=b''.join(struct.pack('>H',v*257) if depth==16 else struct.pack('>f',v/255) for v in raw)
                    channel.set_data(promoted,layer.width,layer.height,depth)
            psd._record.header.depth=depth
            lm=psd._record.layer_and_mask_information
            info=lm.layer_info
            lm.tagged_blocks.set_data(Tag.LAYER_16 if depth==16 else Tag.LAYER_32,
                layer_count=info.layer_count,layer_records=info.layer_records,channel_image_data=info.channel_image_data)
            lm.layer_info=LayerInfo()
        # The library writes all records and channels independently of Graphite.
        # A placeholder preview makes a composite-only implementation fail this test.
        path=root/f'layers-{depth}-{int(compression)}.psd'
        with path.open('wb') as stream: psd._record.write(stream)
        reopened=PSDImage.open(path)
        entries=[]
        for layer in reopened:
            pic=layer.topil().convert('RGBA')
            entries.append(dict(name=layer.name,left=layer.left,top=layer.top,width=pic.width,height=pic.height,
                visible=layer.visible,opacity=layer.opacity,blend=layer.blend_mode.value.decode(),pixels=list(pic.getdata())))
        manifest.append(dict(file=path.name,width=7,height=5,layers=entries))

psd=PSDImage.new('RGBA',(7,5))
group=Group.new(psd,'Folder');group.blend_mode=BlendMode.PASS_THROUGH
layer=PixelLayer.frompil(Image.new('RGBA',(4,3),(20,180,100,255)),group,'Masked',left=1,top=1)
mask=Image.new('L',(4,3));mask.putdata([0,64,128,255]*3)
layer.create_mask(mask,top=1,left=1)
with (root/'group-mask.psd').open('wb') as f:psd._record.write(f)

image=Image.new('RGBA',(7,5));image.putdata([(x*30,y*45,130,40+x*30) for y in range(5) for x in range(7)])
image.save(root/'rgba.PNG')
image.convert('RGB').save(root/'color.bmp')
image.convert('RGB').save(root/'color.jpeg',quality=95)
image.convert('RGB').save(root/'color.jpg',quality=95,progressive=True)
image.convert('P',palette=Image.Palette.ADAPTIVE).save(root/'palette.png')
gray=Image.new('I;16',(7,5));gray.putdata([i*1700 for i in range(35)]);gray.save(root/'gray16.png')
exif=Image.Exif();exif[274]=6
image.convert('RGB').save(root/'rotated.jpg',exif=exif,quality=95)
(root/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False),encoding='utf-8')
print('Created',len(manifest),'independent layered PSD fixtures, group/mask and raster images.')
