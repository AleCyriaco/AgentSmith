import {describe,it,expect} from 'vitest';
import {timeWindow,weeklyWindow} from './repetition';
const at=(day:number,hour:number,minute=0)=>new Date(2026,8,6+day,hour,minute).getTime();
describe('horários de repetição no fuso do Mac',()=>{
 it('usa a janela atual quando já estamos dentro dela',()=>expect(timeWindow('09:00','18:00',new Date(at(0,14)))).toEqual({startsAt:at(0,9),endsAt:at(0,18)}));
 it('agenda amanhã quando o período de hoje terminou',()=>expect(timeWindow('09:00','18:00',new Date(at(0,19)))).toEqual({startsAt:at(1,9),endsAt:at(1,18)}));
 it('atravessa a meia-noite e reconhece a madrugada atual',()=>{
  expect(timeWindow('22:00','02:00',new Date(at(0,23)))).toEqual({startsAt:at(0,22),endsAt:at(1,2)});
  expect(timeWindow('22:00','02:00',new Date(at(0,1)))).toEqual({startsAt:at(-1,22),endsAt:at(0,2)});
  expect(timeWindow('22:00','02:00',new Date(at(0,3)))).toEqual({startsAt:at(0,22),endsAt:at(1,2)});
 });
 it('rejeita horários inválidos ou iguais',()=>{for(const [a,b] of [['24:00','10:00'],['','18:00'],['09:00','09:00']])expect(()=>timeWindow(a,b)).toThrow()});
});

describe('dias da semana',()=>{
 it('seleciona o próximo dia marcado e pula dias não selecionados',()=>{
  expect(weeklyWindow('09:00','18:00',[1,3],new Date(at(0,14)))).toEqual({startsAt:at(1,9),endsAt:at(1,18)});
  expect(weeklyWindow('09:00','18:00',[1,3],new Date(at(1,19)))).toEqual({startsAt:at(3,9),endsAt:at(3,18)});
 });
 it('pula o fim de semana e mantém domingo selecionável',()=>{
  expect(weeklyWindow('09:00','18:00',[1,2,3,4,5],new Date(at(5,19)))).toEqual({startsAt:at(8,9),endsAt:at(8,18)});
  expect(weeklyWindow('09:00','18:00',[7],new Date(at(0,10)))).toEqual({startsAt:at(0,9),endsAt:at(0,18)});
  expect(weeklyWindow('09:00','18:00',[7],new Date(at(0,19)))).toEqual({startsAt:at(7,9),endsAt:at(7,18)});
 });
 it('atribui a madrugada ao dia em que a janela começou',()=>{
  expect(weeklyWindow('22:00','02:00',[1],new Date(at(2,1)))).toEqual({startsAt:at(1,22),endsAt:at(2,2)});
  expect(weeklyWindow('22:00','02:00',[1],new Date(at(2,3)))).toEqual({startsAt:at(8,22),endsAt:at(9,2)});
 });
 it('exige ao menos um dia válido',()=>{for(const days of [[],[0],[8],[1.5]])expect(()=>weeklyWindow('09:00','18:00',days)).toThrow()});
});

describe('período de datas do loop',()=>{
 it('aguarda a data inicial mesmo quando faltam várias semanas',()=>{
  expect(weeklyWindow('09:00','18:00',[1],new Date(at(0,14)),{startDate:'2026-10-05',endDate:'2026-10-31'})).toEqual({startsAt:at(29,9),endsAt:at(29,18)});
 });
 it('inclui o dia final e corta a madrugada na meia-noite final',()=>{
  const dates={startDate:'2026-09-07',endDate:'2026-09-07'};
  expect(weeklyWindow('09:00','18:00',[1],new Date(at(1,10)),dates)).toEqual({startsAt:at(1,9),endsAt:at(1,18)});
  expect(weeklyWindow('22:00','02:00',[1],new Date(at(1,23)),dates)).toEqual({startsAt:at(1,22),endsAt:at(2,0)});
  expect(()=>weeklyWindow('22:00','02:00',[1],new Date(at(2,0)),dates)).toThrow('Não há dia');
 });
 it('não inclui uma madrugada cuja janela começou antes da data inicial',()=>{
  expect(weeklyWindow('22:00','02:00',[1,2],new Date(at(2,1)),{startDate:'2026-09-08'})).toEqual({startsAt:at(2,22),endsAt:at(3,2)});
 });
 it('rejeita período invertido, datas inexistentes, dias ausentes e período encerrado',()=>{
  for(const dates of [{startDate:'2026-09-08',endDate:'2026-09-07'},{startDate:'2026-02-30'},{endDate:'2026-9-07'},{startDate:'2026-09-08',endDate:'2026-09-09'},{endDate:'2026-09-05'}])expect(()=>weeklyWindow('09:00','18:00',[1],new Date(at(0,14)),dates)).toThrow();
 });
});
