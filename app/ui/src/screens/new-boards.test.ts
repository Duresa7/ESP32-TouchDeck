import { beforeEach, describe, expect, test } from 'vitest';
import { newBoard } from '../companion/fake-new-boards.ts';
import { boardCards, boardsUi } from './new-boards.ts';

const a = newBoard({ id: 'mock00aa', port: 'COM7' });
const b = newBoard({ id: 'mock00bb', port: 'COM8', kind: 'silent' });
const ids = (cards: { id: string }[]) => cards.map((c) => c.id);
const none = { ignored: new Set<string>(), started: {} };

describe('the New board cards', () => {
  test('every listed board gets a card', () => {
    expect(boardCards([a, b], [], none)).toEqual({ shown: [a, b], ignored: [] });
  });

  test('an ignored board moves to the chips', () => {
    const cards = boardCards([a, b], [], { ignored: new Set([a.id]), started: {} });
    expect(ids(cards.shown)).toEqual([b.id]);
    expect(ids(cards.ignored)).toEqual([a.id]);
  });

  test('a started install keeps its card while the board turns into a Device', () => {
    const started = { [a.id]: a };
    expect(ids(boardCards([a], [], { ignored: new Set(), started }).shown)).toEqual([a.id]);
    expect(ids(boardCards([], [], { ignored: new Set(), started }).shown)).toEqual([a.id]);
    expect(boardCards([], [{ id: a.id }], { ignored: new Set(), started })).toEqual({ shown: [], ignored: [] });
  });

  test('a started install shows its card even if it was ignored before', () => {
    expect(boardCards([a], [], { ignored: new Set([a.id]), started: { [a.id]: a } })).toEqual({ shown: [a], ignored: [] });
  });
});

describe('the New board choices', () => {
  beforeEach(() => {
    boardsUi.setState({ ignored: new Set(), started: {} });
  });

  test('Ignore, then a click on the chip, brings the card back', () => {
    boardsUi.getState().ignore(a.id);
    expect(boardsUi.getState().ignored).toEqual(new Set([a.id]));
    boardsUi.getState().restore(a.id);
    expect(boardsUi.getState().ignored).toEqual(new Set());
  });

  test('a started install is held until it ends', () => {
    boardsUi.getState().start(a);
    expect(boardsUi.getState().started).toEqual({ [a.id]: a });
    boardsUi.getState().end(a.id);
    expect(boardsUi.getState().started).toEqual({});
  });
});
