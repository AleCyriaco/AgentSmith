import {describe,it,expect,vi} from 'vitest';
import {stopOnEscape} from './stopShortcut';
function key(key='Escape',repeat=false){return {key,repeat,preventDefault:vi.fn(),stopImmediatePropagation:vi.fn()} as unknown as KeyboardEvent}
describe('Esc para parar',()=>{
 it('intercepta Esc antes de enviá-lo à RDP e solicita uma parada',()=>{const event=key(),stop=vi.fn();stopOnEscape(event,true,stop);expect(event.preventDefault).toHaveBeenCalledOnce();expect(event.stopImmediatePropagation).toHaveBeenCalledOnce();expect(stop).toHaveBeenCalledOnce()});
 it('não repete a parada ao segurar Esc',()=>{const event=key('Escape',true),stop=vi.fn();stopOnEscape(event,true,stop);expect(event.preventDefault).toHaveBeenCalledOnce();expect(stop).not.toHaveBeenCalled()});
 it('preserva o teclado manual quando não há execução e ignora outras teclas',()=>{for(const [event,running] of [[key(),false],[key('Enter'),true]] as const){const stop=vi.fn();stopOnEscape(event,running,stop);expect(event.preventDefault).not.toHaveBeenCalled();expect(stop).not.toHaveBeenCalled()}});
});
