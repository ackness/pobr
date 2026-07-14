import { describe, expect, test } from 'vitest';
import { composeNotes, splitNotes } from './annotations';

describe('annotations notes codec', () => {
  test('round-trips overview + multi-line annotations', () => {
    const annotations = {
      'item:weapon1': '主手弓：吃满三条攻速词条\n第二行说明',
      'skill:2': 'Main clear skill',
      'jewel:54127': '范围珠宝，覆盖左侧生命簇',
      'flask:Flask 1': '保命瓶',
    };
    const notes = composeNotes('总览笔记\n第二行', annotations);
    expect(splitNotes(notes)).toEqual({ overview: '总览笔记\n第二行', annotations });
  });

  test('no annotations → notes unchanged; no marker → empty map', () => {
    expect(composeNotes('plain', {})).toBe('plain');
    expect(composeNotes('plain', { 'item:helmet': '  ' })).toBe('plain');
    expect(splitNotes('plain\ntext')).toEqual({ overview: 'plain\ntext', annotations: {} });
  });

  test('empty overview with annotations round-trips', () => {
    const notes = composeNotes('', { 'item:helmet': 'ES 基底' });
    expect(splitNotes(notes)).toEqual({ overview: '', annotations: { 'item:helmet': 'ES 基底' } });
  });
});
