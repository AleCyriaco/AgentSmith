import {describe,it,expect} from 'vitest';
import {activityMessage} from './TaskActivity';
import type {Run} from './types';
const run:Run={id:'test',title:'Teste',machineId:'machine',instructions:'Teste',steps:[],status:'blocked',log:['Motivo do bloqueio'],actionCount:0,updatedAt:0};
describe('andamento da tarefa',()=>{
 it('mostra o bloqueio de tarefas antigas sem progresso detalhado',()=>expect(activityMessage(run)).toBe('Motivo do bloqueio'));
 it('mostra a fase atual em vez de uma mensagem antiga do histórico',()=>expect(activityMessage({...run,status:'verifying',progress:{message:'Conferindo etapa 2',startedAt:5}})).toBe('Conferindo etapa 2'));
 it('não anuncia que ainda está analisando depois de pausar ou concluir',()=>{
  expect(activityMessage({...run,status:'cancelled',progress:{message:'Consultando IA',startedAt:5}})).toContain('encerrada');
  expect(activityMessage({...run,status:'completed',progress:{message:'Consultando IA',startedAt:5}})).toContain('concluídas');
  expect(activityMessage({...run,status:'paused',progress:{message:'Consultando IA',startedAt:5}})).toContain('pausada');
 });
});
