import { drawLayeredScene,drawSceneView } from '../../../platform/ui/widgets/scene-view/widget.js';
import { drawPlot2d } from '../../../platform/ui/widgets/plot-2d/widget.js';
import { drawPlot3d } from '../../../platform/ui/widgets/plot-3d/widget.js';
import { drawColorbar } from '../../../platform/ui/widgets/colorbar/widget.js';
import { fieldColor } from '../../../platform/ui/widgets/shared/plots/field-color.js';
import { getCoreUnit } from '../../../platform/core-rust/js/api.js';
import { fieldQuantityLabel } from './field-surfaces.js';
import { buildApplicationScene,createGeometryProjector,geometryPrimitives,meshPrimitives } from './geometry-scene.js';
import { meshPreviewSurfaces,meshForField } from '../js/mesh-artifact.js';

export function modelProjection({bounds,dimension,width,height,camera={}}) {
  const center=bounds.map(range=>(range.minimum+range.maximum)/2),span=Math.max(1e-12,...bounds.map(range=>range.maximum-range.minimum));
  const scale=.72*Math.min(width,height)/span*(camera.zoom||1),yaw=camera.yaw??.68,pitch=camera.pitch??-.42;
  const rotate=point=>{
    if(dimension===2)return [point[0],-point[1],0];
    const x=Math.cos(yaw)*point[0]-Math.sin(yaw)*point[1],y=Math.sin(yaw)*point[0]+Math.cos(yaw)*point[1];
    return [x,-(Math.cos(pitch)*y-Math.sin(pitch)*point[2]),Math.sin(pitch)*y+Math.cos(pitch)*point[2]];
  };
  const project=point=>{const rotated=rotate([0,1,2].map(axis=>(point[axis]||0)-(center[axis]||0)));return [width/2+(camera.panX||0)+rotated[0]*scale,height/2+(camera.panY||0)+rotated[1]*scale,rotated[2]];};
  return {project,rotate,scale,center};
}

export function drawOrientation({context,width,height,dimension,rotate}) {
  const x=width-34,y=height-30;
  context.save();context.fillStyle='rgba(12,20,30,0.82)';context.beginPath();context.arc(x,y,31,0,2*Math.PI);context.fill();context.lineCap='round';context.lineWidth=1.8;context.font='10px system-ui';
  [['x',[1,0,0],'#ff657a'],['y',[0,1,0],'#60dca6'],['z',[0,0,1],'#75aeff']].slice(0,dimension).forEach(([label,vector,color])=>{
    const projected=rotate(vector),norm=Math.hypot(projected[0],projected[1]);if(norm<1e-12)return;
    const dx=projected[0]/norm*25,dy=projected[1]/norm*25,angle=Math.atan2(dy,dx),tip=[x+dx,y+dy];
    context.strokeStyle=color;context.fillStyle=color;context.beginPath();context.moveTo(x,y);context.lineTo(...tip);
    for(const side of [-1,1]){context.moveTo(...tip);context.lineTo(tip[0]-4.5*Math.cos(angle+side*.55),tip[1]-4.5*Math.sin(angle+side*.55));}context.stroke();context.fillText(label,tip[0]+3,tip[1]-3);
  });
  context.fillStyle='#d5e0ea';context.beginPath();context.arc(x,y,2,0,2*Math.PI);context.fill();context.restore();
}

function savedGeometryModel(model,state){
  if(!state?.isCurrent||!state.symbols)return model;
  return {...model,model:{...model.model,parameters:(model.model.parameters||[]).map(parameter=>state.symbols[parameter.symbol]?{...parameter,expression:String(state.symbols[parameter.symbol].value/getCoreUnit(parameter.unit).scale)}:parameter)}};
}

/** The shared renderer owns drawing kernels; FEM supplies domain projections. */
const projectors=new WeakMap();
export function renderPreview(context={}) {
  const {previewCanvas:canvas,previewContext:ctx,model,selectedPath,camera={},getElement=()=>null}=context;
  if(!canvas||!ctx)return;
  const write=(selector,text)=>{const element=getElement(selector);if(element)element.textContent=text;};
  const bounds=canvas.getBoundingClientRect(),width=Math.max(1,bounds.width),height=Math.max(1,bounds.height);
  try {
    if(!model){drawLayeredScene({canvas,layers:[]});return;}
    const scene=context.activeApplicationPackage.graphicsConsole.scenes[context.activeGraphicsSceneId]||context.activeApplicationPackage.graphicsConsole.scenes.structure;
    const inputs=context.graphicsSceneInputsFor(scene),state=inputs.fieldData;
    const field=scene.onSelect==='loadResult'&&state?.isCurrent?state:null;
    // A structural preview is not evidence for the mesh of a different saved case.
    const mesh=field?meshForField(field.data):inputs.previewMesh;
    const selectedValue=inputs.selectedItem||(Array.isArray(selectedPath)?context.valueAt?.(selectedPath):null);
    if(!projectors.has(canvas))projectors.set(canvas,createGeometryProjector());
    const projection=buildApplicationScene({model:savedGeometryModel(model,field),selectedPath,selectedValue,mesh,expressionPolicy:context.expressionPolicy,visibility:{geometry:false},projector:projectors.get(canvas)});
    const allPoints=[...(mesh?.nodes||[]),...(field?.data.nodes||[])];
    for(let axis=0;axis<projection.dimension;axis++)for(const point of allPoints){projection.bounds[axis].minimum=Math.min(projection.bounds[axis].minimum,point[axis]);projection.bounds[axis].maximum=Math.max(projection.bounds[axis].maximum,point[axis]);}
    if(projection.bounds.some(range=>!Number.isFinite(range.minimum)||!Number.isFinite(range.maximum)))throw new Error(projection.diagnostics[0]?.message||'There is no drawable geometry.');
    const view=modelProjection({...projection,width,height,camera}),visible=role=>context.graphicsVisibility?.(role)??true;
    const layers=[];
    for(const layer of scene.layers||[]){
      if(layer.control&&!visible(layer.role))continue;
      if((layer.requiresVisible||[]).some(role=>!visible(role)))continue;
      if(layer.role==='field'&&field)layers.push({id:layer.id,draw:projection.dimension===2?drawPlot2d:drawPlot3d,options:{data:{surfaces:field.surfaces,range:field.range,opacity:.88},color:fieldColor}});
      else if(layer.role==='mesh'&&mesh)layers.push({id:layer.id,draw:drawSceneView,options:{scene:{dimension:projection.dimension,primitives:meshPrimitives(mesh,{field:Boolean(field),edges:visible('edges')})}}});
      else if(layer.role==='geometry')layers.push({id:layer.id,draw:drawSceneView,options:{scene:{dimension:projection.dimension,primitives:geometryPrimitives(projection)}}});
      else if(layer.role==='selection')layers.push({id:layer.id,draw:drawSceneView,options:{scene:{dimension:projection.dimension,primitives:projection.primitives}}});
    }
    layers.push({id:'annotations',draw:({context:drawContext})=>{
      if(field)drawColorbar({context:drawContext,width,height,range:field.range,label:fieldQuantityLabel(field.quantity||field.action?.quantity),units:field.unit,color:fieldColor});
      drawOrientation({context:drawContext,width,height,dimension:projection.dimension,rotate:view.rotate});
    }});
    drawLayeredScene({canvas,layers,project:view.project,view:camera});
    canvas.setAttribute('aria-label',`${projection.dimension}D ${field?'field result':'geometry'} viewport`);
    write('#geometry-note',projection.dimension===2?'Drag to pan · wheel to zoom · orientation axes at lower right.':'Drag to rotate · wheel to zoom · orientation axes at lower right.');
    const meshCount=mesh&&visible('mesh')?meshPreviewSurfaces(mesh):null;
    const summary=field?`${field.action.name} · ${context.fieldPointLabel?.(field)||'checked case'}`:
      `${visible('geometry')?`${projection.features.length} geometry feature(s)`:'Geometry hidden'}${meshCount?` · preview draws ${meshCount.surfaces.length} of ${meshCount.total} mesh faces`:''}`;
    const highlight=projection.selectedGeometryId?` · highlighted ${selectedValue?.name||projection.selectedGeometryId}`:projection.selectionPaint?.primitives.length?` · highlighted ${projection.selectionPaint.selections.map(selection=>`${selection.name||selection.id} (${selection.entity}, ${selection.source})`).join(', ')}`:'';
    write('#preview-summary',`${summary}${highlight}${projection.diagnostics.length?` · ${projection.diagnostics.map(issue=>issue.message).join(' ')}`:''}`);
    return {scene:projection,layers:layers.map(layer=>layer.id),summary};
  }catch(error){
    drawLayeredScene({canvas,layers:[]});write('#preview-summary',error.message);canvas.setAttribute('aria-label','Geometry unavailable');return {error:error.message};
  }
}
