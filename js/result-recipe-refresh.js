/** Refresh checked tabular data after a committed recipe edit, without a solve. */
export function createResultRecipeRefresh(){
  let previous=null,revision=0;
  return {
    reset(){previous=null;revision++;},
    observe(action,reload){
      if(!action||!['plot.sweep','sweep'].includes(action.type)){this.reset();return null;}
      const next={id:action.id,recipe:JSON.stringify(action)};
      const changed=previous?.id===next.id&&previous.recipe!==next.recipe;
      if(previous?.id!==next.id||previous.recipe!==next.recipe)revision++;
      const ticket=revision;
      previous=next;
      if(!changed)return null;
      // Run after the host's synchronous Apply/redraw, coalescing rapid edits.
      // Store the new recipe before reload, which itself renders Settings.
      return Promise.resolve().then(()=>ticket===revision?reload(action):undefined);
    },
  };
}
