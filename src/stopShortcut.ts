export function stopOnEscape(event:KeyboardEvent,running:boolean,stop:()=>void){
 if(event.key!=='Escape'||!running)return;
 event.preventDefault();event.stopImmediatePropagation();
 if(!event.repeat)stop();
}
