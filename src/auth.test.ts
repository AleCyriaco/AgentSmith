import {describe,it,expect} from 'vitest';
import {withAuth,loginClients} from './auth';
import {vendors} from './catalog';
import type {Profile} from './types';
describe('provider authentication',()=>{
 it('switches all four official providers without retaining an arbitrary API endpoint or model',()=>{
  for(const id of Object.keys(loginClients)){
   const v=vendors.find(v=>v.id===id)!;
   const p:Profile={id:'1',vendor:id,name:'My model',protocol:v.protocol,baseUrl:'https://old.example',model:'api-only-model',vision:true,enabled:true};
   const login=withAuth(p,v,'browser');
   expect(login).toMatchObject({id:'1',name:'My model',authMethod:'browser',baseUrl:`official://${id}`,model:'default',vision:false});
   expect(withAuth(login,v,'api_key')).toMatchObject({authMethod:'api_key',baseUrl:v.baseUrl,model:'',vision:false});
  }
 });
});
