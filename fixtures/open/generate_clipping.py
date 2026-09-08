import sys
from pathlib import Path
from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import PixelLayer
root=Path(__file__).resolve().parent
for translucent in (False,True):
 p=PSDImage.new('RGBA',(7,5))
 im=Image.new('RGBA',(7,5));im.putdata([(100,150,200,(128 if translucent else 255) if x<4 else 0) for y in range(5) for x in range(7)])
 base=PixelLayer.frompil(im,p,'Clip base')
 top=PixelLayer.frompil(Image.new('RGBA',(7,5),(240,40,20,255)),p,'Clipped color');top.clipping=True
 with (root/('clip-translucent.psd' if translucent else 'clip-opaque.psd')).open('wb') as f:p._record.write(f)
print('Created clipping fixtures')
