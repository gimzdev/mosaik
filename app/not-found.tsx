import Link from 'next/link';

export default function NotFound() {
  return (
    <div className="page-x pb-24 pt-40">
      <h1 className="h1">That window isn’t here.</h1>
      <p className="mt-5 max-w-md text-lg text-slate">The page you asked for doesn’t exist. Head back to the start.</p>
      <Link href="/" className="btn btn-lapis mt-8">
        Go to the homepage
      </Link>
    </div>
  );
}
