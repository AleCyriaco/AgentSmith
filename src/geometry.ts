export function imagePoint(clientX:number,clientY:number,rect:{left:number;top:number;width:number;height:number},width:number,height:number){
 const scale=Math.min(rect.width/width,rect.height/height);if(!Number.isFinite(scale)||scale<=0)return null;
 const x=(clientX-rect.left-(rect.width-width*scale)/2)/scale,y=(clientY-rect.top-(rect.height-height*scale)/2)/scale;
 return x>=0&&y>=0&&x<width&&y<height?{x:Math.floor(x),y:Math.floor(y)}:null;
}

export function viewScale(viewWidth:number,viewHeight:number,width:number,height:number,zoom:string){
 if(width<=0||height<=0)return 1;
 if(zoom==='fit')return Math.max(0.01,Math.min(1,viewWidth/width,viewHeight/height));
 const value=Number(zoom);return [50,75,100,125,150,200].includes(value)?value/100:1;
}
