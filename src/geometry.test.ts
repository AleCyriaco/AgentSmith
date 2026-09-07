import {describe,it,expect} from 'vitest';
import {imagePoint,viewScale} from './geometry';
describe('remote coordinates',()=>{it('removes letterboxing and rejects the black margins',()=>{const r={left:10,top:20,width:1000,height:1000};expect(imagePoint(510,520,r,1000,500)).toEqual({x:500,y:250});expect(imagePoint(510,100,r,1000,500)).toBeNull();expect(imagePoint(1010,520,r,1000,500)).toBeNull();});it('maps scaled monitors',()=>{expect(imagePoint(640,400,{left:0,top:0,width:640,height:400},1280,800)).toBeNull();expect(imagePoint(320,200,{left:0,top:0,width:640,height:400},1280,800)).toEqual({x:640,y:400});});});

it('fits without enlarging and maps scrolled zoomed image coordinates',()=>{
 expect(viewScale(3000,2000,1600,900,'fit')).toBe(1);
 expect(viewScale(800,450,1600,900,'fit')).toBe(0.5);
 expect(viewScale(800,450,1600,900,'150')).toBe(1.5);
 expect(imagePoint(400,250,{left:-200,top:-50,width:2400,height:1350},1600,900)).toEqual({x:400,y:200});
});
