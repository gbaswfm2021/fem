import {evaluateQuantity,evaluateQuantityExpression} from '../../../platform/core-rust/js/api.js';
import {decodeSelection} from './selection-adapter.js';
import {selectionGeometrySignature} from '../js/mesh-map.js';

const kinds=new Set(['rectangle','circle','box','cylinder','union','intersection']);
const add=(a,b)=>a.map((value,index)=>value+b[index]),sub=(a,b)=>a.map((value,index)=>value-b[index]);
const mul=(a,value)=>a.map(coordinate=>coordinate*value),dot=(a,b)=>a.reduce((value,coordinate,index)=>value+coordinate*b[index],0);
const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],norm=a=>Math.hypot(...a);
const center=points=>mul(points.reduce(add,[0,0,0]),1/points.length),lerp=(a,b,t)=>add(a,mul(sub(b,a),t));

export function selectionFocus(model,selected){
  if(!selected)return [];
  const selections=model.model.selections||[],find=id=>{
    const matches=selections.filter(selection=>selection.id===id);if(matches.length!==1)throw new Error(`Selection ${id} is missing or ambiguous.`);return matches[0];
  };
  if(selected.entity&&Array.isArray(selected.addresses))return [selected];
  if(selected.selectionId)return [find(selected.selectionId)];
  let pair=selected;
  if(selected.periodicPairId){const matches=(model.model.mesh?.periodicPairs||[]).filter(pair=>pair.id===selected.periodicPairId);if(matches.length!==1)throw new Error(`Periodic pair ${selected.periodicPairId} is missing or ambiguous.`);pair=matches[0];}
  if(pair.lowerSelectionId&&pair.upperSelectionId)return [...new Set([pair.lowerSelectionId,pair.upperSelectionId])].map(find);
  return [];
}

function inFace(face,point,epsilon){
  return Math.abs(dot(face.normal,point)-face.offset)<=epsilon&&face.vertices.every((a,index)=>dot(cross(sub(face.vertices[(index+1)%face.vertices.length],a),sub(point,a)),face.normal)>=-epsilon*norm(sub(face.vertices[(index+1)%face.vertices.length],a)));
}
function rayTriangle(point,direction,a,b,c,epsilon){
  const u=sub(b,a),v=sub(c,a),h=cross(direction,v),det=dot(u,h);if(Math.abs(det)<1e-15*norm(u)*norm(v))return null;
  const relative=sub(point,a),s=dot(relative,h)/det,q=cross(relative,u),t=dot(direction,q)/det,distance=dot(v,q)/det;
  return s>=-1e-10&&t>=-1e-10&&s+t<=1+1e-10&&distance>epsilon?distance:null;
}
function classify(entry,point,dimension,epsilon){
  const probe=dimension===2?[point[0],point[1],epsilon*4]:point;
  const faces=dimension===2?entry.faces.filter(face=>Math.abs(face.normal[2])<1-1e-10):entry.faces;
  if(faces.some(face=>inFace(face,probe,epsilon)))return 'boundary';
  const direction=dimension===2?[1,.173,0]:[1,.173,.317],hits=[];
  for(const face of entry.faces)for(let index=1;index<face.vertices.length-1;index++){
    const distance=rayTriangle(probe,direction,face.vertices[0],face.vertices[index],face.vertices[index+1],epsilon);
    if(distance!==null&&!hits.some(hit=>Math.abs(hit-distance)<epsilon))hits.push(distance);
  }
  return hits.length%2?'inside':'outside';
}

function coordinatePlane(selection,model,epsilon,projection){
  if(!selection.coordinateExpression)return null;
  const parameters=projection.parameters,policy=projection.expressionPolicy,symbols=Object.fromEntries(Object.entries(parameters).map(([name,value])=>[name,{...value,dimension:[0,0,0,0,value.dimension[4]]}]));
  const evaluate=point=>{
    const result=evaluateQuantityExpression(selection.coordinateExpression,{...symbols,...Object.fromEntries(['x','y','z'].map((axis,index)=>[axis,{value:model.scenario.dimension===2&&index===2?0:point[index],dimension:[0,0,0,0,0]}]))},'Selection coordinate expression',policy?.allowedFunctions||[],policy);
    if(Math.abs(result.dimension[4])>1e-10)throw new Error('A geometric coordinate expression must not have temperature units.');return result.value;
  };
  const origin=evaluate([0,0,0]),gradient=[0,1,2].map(axis=>evaluate([0,0,0].map((_,index)=>index===axis?1:0))-origin),length=norm(gradient);
  const probes=[[.23,.47,.81],[-.31,.72,.18],[1.13,-.29,.54]];
  if(probes.some(point=>Math.abs(evaluate(point)-origin-dot(gradient,point))>1e-10*Math.max(1e-12,Math.abs(evaluate(point)),Math.abs(origin+dot(gradient,point)))))throw new Error('The coordinate zero set must be an affine plane for exact CAD selection.');
  if(length<=1e-14){if(Math.abs(origin)<=Math.min(epsilon,1e-12))return null;throw new Error('The coordinate expression has no affine zero plane.');}
  return {normal:mul(gradient,1/length),offset:-origin/length};
}

function splitVertices(vertices,plane,epsilon){
  const distances=vertices.map(vertex=>dot(plane.normal,vertex)-plane.offset);
  if(!distances.some(value=>value>epsilon)||!distances.some(value=>value<-epsilon))return [vertices];
  const sides=[[],[]];
  for(let index=0;index<vertices.length;index++){
    const a=vertices[index],b=vertices[(index+1)%vertices.length],left=distances[index],right=distances[(index+1)%vertices.length];
    if(left>=-epsilon)sides[0].push(a);if(left<=epsilon)sides[1].push(a);
    if(left>epsilon&&right<-epsilon||left<-epsilon&&right>epsilon){const intersection=lerp(a,b,left/(left-right));sides[0].push(intersection);sides[1].push(intersection);}
  }
  return sides.filter(side=>side.length>=3);
}
function clipToFace(vertices,face,epsilon){
  let result=vertices;
  for(let index=0;index<face.vertices.length&&result.length;index++){
    const a=face.vertices[index],b=face.vertices[(index+1)%face.vertices.length],inward=cross(face.normal,sub(b,a));
    const plane={normal:inward,offset:dot(inward,a)},output=[];
    for(let j=0;j<result.length;j++){
      const left=result[j],right=result[(j+1)%result.length],dl=dot(inward,left)-plane.offset,dr=dot(inward,right)-plane.offset;
      if(dl>=-epsilon)output.push(left);if(dl>epsilon&&dr<-epsilon||dl<-epsilon&&dr>epsilon)output.push(lerp(left,right,dl/(dl-dr)));
    }
    result=output;
  }
  return result;
}

function faceIntersection(left,right,epsilon){
  const direction=cross(left.normal,right.normal),squared=dot(direction,direction);if(squared<1e-20)return null;
  const point=mul(add(mul(cross(right.normal,direction),left.offset),mul(cross(direction,left.normal),right.offset)),1/squared);
  let lower=-Infinity,upper=Infinity;
  for(const face of [left,right])for(let index=0;index<face.vertices.length;index++){
    const a=face.vertices[index],inward=cross(face.normal,sub(face.vertices[(index+1)%face.vertices.length],a)),position=dot(inward,sub(point,a)),slope=dot(inward,direction);
    if(Math.abs(slope)<1e-15){if(position<-epsilon*norm(inward))return null;continue;}
    const limit=-position/slope;if(slope>0)lower=Math.max(lower,limit);else upper=Math.min(upper,limit);
    if(lower>upper)return null;
  }
  return Number.isFinite(lower)&&Number.isFinite(upper)&&upper-lower>epsilon/norm(direction)?[add(point,mul(direction,lower)),add(point,mul(direction,upper))]:null;
}

function faceGrid(face,dimension){
  const vertices=face.vertices,patches=[],lines=[];
  if(vertices.length===4){
    const across=face.surface.includes(':wall')?1:8,along=8;
    const at=(u,v)=>lerp(lerp(vertices[0],vertices[1],u),lerp(vertices[3],vertices[2],u),v);
    for(let u=0;u<across;u++)for(let v=0;v<along;v++)patches.push([at(u/across,v/along),at((u+1)/across,v/along),at((u+1)/across,(v+1)/along),at(u/across,(v+1)/along)]);
    for(let u=0;u<=across;u++)lines.push([at(u/across,0),at(u/across,1)]);
    for(let v=0;v<=along;v++)lines.push([at(0,v/along),at(1,v/along)]);
  }else if(face.surface.includes(':cap:')){
    const middle=center(vertices);
    for(let angle=0;angle<vertices.length;angle++)for(let radial=0;radial<4;radial++){
      const a=vertices[angle],b=vertices[(angle+1)%vertices.length];
      patches.push(radial===0?[middle,lerp(middle,a,.25),lerp(middle,b,.25)]:[lerp(middle,a,radial/4),lerp(middle,a,(radial+1)/4),lerp(middle,b,(radial+1)/4),lerp(middle,b,radial/4)]);
      lines.push([lerp(middle,a,(radial+1)/4),lerp(middle,b,(radial+1)/4)]);
    }
    for(const vertex of vertices)lines.push([middle,vertex]);
  }else patches.push(vertices);
  return {patches:dimension===2?[vertices]:patches,lines:dimension===2?[]:lines};
}

function mergeStrokes(primitives,epsilon){
  const result=[];
  for(const primitive of primitives){
    if(primitive.type!=='polyline'||primitive.points.length!==2){result.push(primitive);continue;}
    let points=primitive.points.map(point=>[point[0],point[1],point[2]||0]),changed=true;
    while(changed){changed=false;const direction=mul(sub(points[1],points[0]),1/norm(sub(points[1],points[0])));
      for(let index=0;index<result.length;index++){
        const existing=result[index];if(existing.type!=='polyline'||existing.lineWidth!==primitive.lineWidth||existing.selectionId!==primitive.selectionId||existing.opacity!==primitive.opacity)continue;
        const other=existing.points.map(point=>[point[0],point[1],point[2]||0]);
        if(other.some(point=>norm(cross(sub(point,points[0]),direction))>epsilon))continue;
        const interval=other.map(point=>dot(sub(point,points[0]),direction)).sort((a,b)=>a-b),length=norm(sub(points[1],points[0]));
        if(interval[1]<-epsilon||interval[0]>length+epsilon)continue;
        points=[add(points[0],mul(direction,Math.min(0,interval[0]))),add(points[0],mul(direction,Math.max(length,interval[1])))];result.splice(index,1);changed=true;break;
      }
    }
    result.push({...primitive,points:points.map(point=>point.slice(0,primitive.points[0].length))});
  }
  return result;
}

function selectionOutlines(faces,epsilon){
  const groups=new Map();
  for(const face of faces)for(let index=0;index<face.vertices.length;index++){
    const a=face.vertices[index],b=face.vertices[(index+1)%face.vertices.length],length=norm(sub(b,a));if(length<=epsilon)continue;
    let direction=mul(sub(b,a),1/length);if(direction.find(value=>Math.abs(value)>1e-10)<0)direction=mul(direction,-1);
    const moment=cross(a,direction),key=direction.map(value=>Math.round(value/1e-10)).join(',')+'|'+moment.map(value=>Math.round(value/epsilon)).join(',');
    if(!groups.has(key))groups.set(key,{direction,anchor:a,events:[]});
    const group=groups.get(key),values=[dot(sub(a,group.anchor),group.direction),dot(sub(b,group.anchor),group.direction)].sort((a,b)=>a-b),id=Symbol();
    group.events.push({at:values[0],add:true,face,id},{at:values[1],add:false,face,id});
  }
  const lines=[];
  for(const {events,direction,anchor}of groups.values()){
    events.sort((a,b)=>a.at-b.at);const active=new Map();let index=0,previous=events[0].at;
    while(index<events.length){
      const position=events[index].at,faces=[...active.values()];
      if(position-previous>epsilon&&(faces.length===1||faces.some(face=>face.surface!==faces[0].surface&&Math.abs(dot(face.normal,faces[0].normal))<1-1e-10)))lines.push([add(anchor,mul(direction,previous)),add(anchor,mul(direction,position))]);
      while(index<events.length&&Math.abs(events[index].at-position)<=epsilon){const event=events[index++];if(event.add)active.set(event.id,event.face);else active.delete(event.id);}
      previous=position;
    }
  }
  return lines;
}

export function checkedSelectionPrimitives(mesh,selection,dimension,accent,ridges=[]){
  const tag=mesh?.selectionTags?.[selection.id];
  if(!tag||tag.entity!==selection.entity||mesh.selectionSignatures?.[selection.id]!==selectionGeometrySignature(selection))return null;
  const types=selection.entity==='point'?['P']:selection.entity==='edge'?['L']:dimension===2?['E','C']:['F'];
  const records=(mesh.records||[]).filter(record=>types.includes(record.type)&&record.tags[0]===tag.tag);
  const primitives=records.map(record=>({
    type:selection.entity==='point'?'point':selection.entity==='boundary'&&dimension===3?'triangle':'polyline',points:record.nodes.map(node=>mesh.nodes[node].slice(0,dimension)),
    ...(selection.entity==='point'?{fill:accent,lineWidth:10/3}:selection.entity==='boundary'&&dimension===3?{fill:accent,stroke:'none',opacity:.30}:{stroke:accent,lineWidth:4.5,opacity:1}),selectionId:selection.id,selectionSource:'checked mesh',
  }));
  if(selection.entity==='boundary'&&dimension===3){
    const faces=records.map(record=>{
      const vertices=record.nodes.map(node=>mesh.nodes[node].slice(0,3)),normal=cross(sub(vertices[1],vertices[0]),sub(vertices[2],vertices[0])),length=norm(normal);
      return {vertices,normal:length?mul(normal,1/length):normal,surface:'mesh'};
    });
    const epsilon=1e-12,lines=selectionOutlines(faces,epsilon);
    // The public mesh carries physical tags, not CAD-face identities. Obtain
    // real ridges from CAD and clip them to the checked selected triangles.
    for(const [a,b]of ridges)for(const face of faces){
      if(norm(face.normal)===0||[a,b].some(point=>Math.abs(dot(face.normal,sub(point,face.vertices[0])))>epsilon))continue;
      const direction=sub(b,a);let lower=0,upper=1;
      for(let index=0;index<3;index++){
        const vertex=face.vertices[index],inward=cross(face.normal,sub(face.vertices[(index+1)%3],vertex));
        const position=dot(inward,sub(a,vertex)),slope=dot(inward,direction);
        if(Math.abs(slope)<1e-15){if(position<-epsilon*norm(inward)){upper=-1;break;}}
        else if(slope>0)lower=Math.max(lower,-position/slope);else upper=Math.min(upper,-position/slope);
      }
      if(upper>lower&&norm(direction)*(upper-lower)>epsilon)lines.push([lerp(a,b,lower),lerp(a,b,upper)]);
    }
    const unique=new Map();for(const points of lines){const key=points.map(point=>point.map(value=>Math.round(value/epsilon)).join(',')).sort().join('|');unique.set(key,points);}
    for(const points of unique.values())primitives.push({type:'polyline',points,stroke:accent,opacity:.82,lineWidth:.8,
      selectionId:selection.id,selectionSource:'checked mesh'});
  }
  return primitives;
}

/** Resolve sparse addresses against the same polygonal CAD used for contours. */
export function namedSelectionPrimitives({model,projection,selected,mesh,accent='#48d7b2'}){
  const selections=selectionFocus(model,selected),primitives=[],diagnostics=[],dimension=projection.dimension;
  if(!selections.length)return {primitives,diagnostics,selectedPatches:[]};
  const policy=projection.expressionPolicy,epsilon=Math.max(1e-12,evaluateQuantity(model.model.geometry.tolerance||{expression:'1e-9',unit:'m'},projection.parameters,'Geometry tolerance',policy?.allowedFunctions||[],policy)),entries=projection.features.filter(entry=>kinds.has(entry.type)),planes=[];
  for(const entry of entries)for(const face of entry.faces)if(!planes.some(plane=>norm(sub(plane.normal,face.normal))<1e-10&&Math.abs(plane.offset-face.offset)<epsilon))planes.push(face);
  const selectedPatches=[];
  for(const selection of selections){
    const decoded=decodeSelection(selection,model.model.geometry.features);if(decoded.diagnostics.length){diagnostics.push(...decoded.diagnostics);continue;}
    const unscoped=decoded.addressRows.flatMap(row=>Object.keys(row.membership)).filter(id=>!entries.some(entry=>entry.id===id));
    if(unscoped.length){diagnostics.push({message:`CAD address scope does not include nested feature ${[...new Set(unscoped)].join(', ')}.`});continue;}
    const curvedPoint=selection.entity==='point'&&decoded.addressRows.some(row=>entries.some(entry=>['circle','cylinder'].includes(entry.type)&&row.membership[entry.id]==='boundary'));
    if(curvedPoint){
      const checked=checkedSelectionPrimitives(mesh,selection,dimension,accent,projection.features.flatMap(entry=>entry.outlines));
      if(checked?.length)primitives.push(...checked);else diagnostics.push({message:checked?'The checked mesh has no points matching this selection.':'Load the checked mesh and selection map to highlight points on circular CAD.'});
      continue;
    }
    let filter;
    try{filter=coordinatePlane(selection,model,epsilon,projection);}
    catch(error){
      if(!error.message.includes('must be an affine plane'))throw error;
      const checked=checkedSelectionPrimitives(mesh,selection,dimension,accent,projection.features.flatMap(entry=>entry.outlines));
      if(!checked?.length)throw error;
      primitives.push(...checked);continue;
    }
    const matches=point=>decoded.addressRows.some(row=>entries.every(entry=>classify(entry,point,dimension,epsilon)===(row.membership[entry.id]||'outside')));
    const accepted=(point,normal)=>selection.entity==='domain'&&dimension===3?matches(add(point,mul(normal,epsilon*8)))!==matches(add(point,mul(normal,-epsilon*8))):matches(point);
    const painted=[];
    if(['domain','boundary'].includes(selection.entity)&&!(dimension===2&&selection.entity==='boundary')){
      const seen=new Set();
      for(const entry of projection.features)for(const face of entry.faces){
        if(dimension===2&&!face.vertices.every(point=>Math.abs(point[2])<=epsilon))continue;
        if(filter&&!face.vertices.every(point=>Math.abs(dot(filter.normal,point)-filter.offset)<=epsilon))continue;
        const origin=projection.byId.get(face.surface.split(':')[0]),source=origin?.faces.find(candidate=>candidate.surface===face.surface&&norm(sub(candidate.normal,face.normal))<1e-8&&Math.abs(candidate.offset-face.offset)<epsilon)||face;
        const grid=faceGrid(source,dimension);
        for(const patch of grid.patches){
          const clipped=clipToFace(patch,face,epsilon);if(clipped.length<3)continue;
          let pieces=[clipped];for(const plane of planes){pieces=pieces.flatMap(vertices=>splitVertices(vertices,plane,epsilon));if(pieces.length>10000)throw new Error('The selection CAD partition is too large to display.');}
          let selectedPatch;
          for(const vertices of pieces){const middle=center(vertices);if(!accepted(middle,face.normal))continue;
            const key=vertices.map(point=>point.map(value=>Math.round(value/epsilon)).join(',')).sort().join('|');if(seen.has(key))continue;seen.add(key);
            const paintedFace={...face,vertices};painted.push(paintedFace);
            if(!selectedPatch){selectedPatch={selectionId:selection.id,surface:face.surface,gridVertices:clipped,polygons:[]};selectedPatches.push(selectedPatch);}selectedPatch.polygons.push(vertices);
            for(let index=1;index<vertices.length-1;index++)primitives.push({type:'triangle',points:[vertices[0],vertices[index],vertices[index+1]].map(point=>point.slice(0,dimension)),fill:accent,stroke:'none',opacity:dimension===2?.24:.30,selectionId:selection.id});
          }
        }
        for(const line of grid.lines){
          const fractions=[0,1];for(const plane of planes){const a=dot(plane.normal,line[0])-plane.offset,b=dot(plane.normal,line[1])-plane.offset;if(Math.abs(a-b)>1e-14){const t=a/(a-b);if(t>0&&t<1)fractions.push(t);}}
          fractions.sort((a,b)=>a-b);
          for(let index=1;index<fractions.length;index++){
            const a=lerp(...line,fractions[index-1]),b=lerp(...line,fractions[index]),middle=lerp(a,b,.5);
            if(norm(sub(a,b))>epsilon&&inFace(face,middle,epsilon)&&accepted(middle,face.normal))primitives.push({type:'polyline',points:[a,b].map(point=>point.slice(0,dimension)),stroke:accent,opacity:.82,lineWidth:.8,selectionId:selection.id});
          }
        }
      }
      for(const line of selectionOutlines(painted,epsilon))primitives.push({type:'polyline',points:line.map(point=>point.slice(0,dimension)),stroke:accent,opacity:dimension===2?1:.82,lineWidth:dimension===2?2.8:.8,selectionId:selection.id});
    }else{
      const lines=entries.flatMap(entry=>entry.outlines),points=[];
      if(dimension===3)for(let left=0;left<entries.length;left++)for(let right=left+1;right<entries.length;right++){
        if(!decoded.addressRows.some(row=>row.membership[entries[left].id]==='boundary'&&row.membership[entries[right].id]==='boundary'))continue;
        for(const a of entries[left].faces)for(const b of entries[right].faces){const intersection=faceIntersection(a,b,epsilon);if(intersection)lines.push(intersection);}
      }
      for(const line of lines){
        const fractions=[0,1];for(const plane of [...planes,...(filter?[filter]:[])]){const a=dot(plane.normal,line[0])-plane.offset,b=dot(plane.normal,line[1])-plane.offset;if(Math.abs(a-b)>1e-14){const t=a/(a-b);if(t>0&&t<1)fractions.push(t);}}
        fractions.sort((a,b)=>a-b);for(const t of fractions)points.push(lerp(...line,t));
        if(selection.entity==='point')continue;
        for(let index=1;index<fractions.length;index++){const a=lerp(...line,fractions[index-1]),b=lerp(...line,fractions[index]),middle=lerp(a,b,.5);
          if(norm(sub(a,b))>epsilon&&matches(middle)&&(!filter||[a,b].every(point=>Math.abs(dot(filter.normal,point)-filter.offset)<=epsilon)))primitives.push({type:'polyline',points:[a,b].map(point=>point.slice(0,dimension)),stroke:accent,lineWidth:4.5,opacity:1,selectionId:selection.id});
        }
      }
      if(selection.entity==='point'){const seen=new Set();for(const point of points){const key=point.map(value=>Math.round(value/epsilon)).join(',');if(!seen.has(key)&&matches(point)&&(!filter||Math.abs(dot(filter.normal,point)-filter.offset)<=epsilon)){seen.add(key);primitives.push({type:'point',points:[point.slice(0,dimension)],fill:accent,lineWidth:10/3,selectionId:selection.id});}}}
    }
    if(!primitives.some(primitive=>primitive.selectionId===selection.id)){
      const checked=checkedSelectionPrimitives(mesh,selection,dimension,accent,projection.features.flatMap(entry=>entry.outlines));if(checked?.length)primitives.push(...checked);else diagnostics.push({message:`Selection ${selection.name||selection.id} matches no ${checked?'checked mesh':'CAD'} entities.`});
    }
  }
  return {primitives:mergeStrokes(primitives,epsilon),diagnostics,selectedPatches,selections:selections.map(({id,name,entity})=>({id,name,entity,source:primitives.some(primitive=>primitive.selectionId===id&&primitive.selectionSource)?'checked mesh':'CAD'}))};
}
