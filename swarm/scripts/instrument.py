from pathlib import Path
src = Path('swarm/scripts/portal_logic_acceptance.mjs').read_text()
anchor = '    await page.waitForTimeout(1800); // row flash animation after edit must finish before a stable click.'
log = '''    await page.waitForTimeout(1800); // row flash animation after edit must finish before a stable click.
    {
      const b = page.locator('tr').filter({ hasText: `${prefix}-provider-edited` }).getByRole('button', { name: 'Delete' });
      const info = await b.evaluate(el => { const r=el.getBoundingClientRect(); const at=document.elementFromPoint(r.x+r.width/2, r.y+r.height/2); return { disabled: el.disabled, x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height), coveredBy: at ? (at.tagName+'.'+String(at.className).slice(0,40)) : 'none', hasOnclick: !!el.onclick }; }).catch(e => ({err: e.message}));
      console.log('DELETEBTN_STATE', JSON.stringify(info));
    }
'''
src = src.replace(anchor, log, 1)
Path('swarm/scripts/portal_logic_acceptance_instrumented.mjs').write_text(src)
print('instrumented')