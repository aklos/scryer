// Asset imports the Remotion bundler resolves.
declare module "*.css";
declare module "*.png" {
  const url: string;
  export default url;
}
